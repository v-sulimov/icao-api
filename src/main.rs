use actix_web::{get, middleware::Logger, web, App, HttpResponse, HttpServer, ResponseError};
use log::info;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Maximum number of items that can be returned in a single page response.
/// Requests specifying a limit higher than this value will be clamped to this maximum.
const MAX_PAGE_LIMIT: usize = 50;

/// Generic structure for paginated API responses with lifetime parameters
/// enabling zero-copy data access through slice operations.
///
/// # Type Parameters
/// - `'a`: Lifetime parameter ensuring data references remain valid
/// - `T`: Type of the items being paginated
#[derive(Debug, Serialize)]
pub struct PaginatedResponse<'a, T> {
    /// Total number of elements available across all pages
    pub total: usize,
    /// Flag indicating if more results are available beyond current page
    pub has_more: bool,
    /// Number of elements remaining after current page
    pub remaining: usize,
    /// Slice containing the current page's data
    pub data: &'a [T],
}

/// Efficiently paginates a dataset using slice operations without data copying.
///
/// # Parameters
/// - `data`: The complete dataset to paginate
/// - `offset`: Optional starting index (0-based, clamped to data length)
/// - `limit`: Optional maximum items per page (clamped to MAX_PAGE_LIMIT)
///
/// # Returns
/// `PaginatedResponse` containing:
/// - Calculated pagination metadata
/// - Slice reference to the requested data page
///
/// # Behavior
/// - Offset defaults to 0 if not specified
/// - Limit defaults to remaining items after offset if not specified
/// - Automatically clamps values to valid ranges and maximum page size
fn paginate<T>(data: &[T], offset: Option<usize>, limit: Option<usize>) -> PaginatedResponse<T> {
    let total = data.len();
    let start = offset.unwrap_or(0).min(total);
    let requested = limit.unwrap_or(total.saturating_sub(start));
    let limit = requested.min(MAX_PAGE_LIMIT);
    let end = (start + limit).min(total);

    PaginatedResponse {
        total,
        has_more: end < total,
        remaining: total.saturating_sub(end),
        data: &data[start..end],
    }
}

/// Represents airport information with precomputed lowercase fields
/// for efficient case-insensitive searching.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Airport {
    /// Official ICAO code (e.g., "KJFK")
    pub icao: String,
    /// Full airport name (e.g., "John F. Kennedy International Airport")
    pub name: String,

    /// Lowercase version of ICAO code for efficient searching
    #[serde(skip_serializing, skip_deserializing)]
    lower_icao: String,
    /// Lowercase version of name for efficient searching
    #[serde(skip_serializing, skip_deserializing)]
    lower_name: String,
}

/// Intermediate structure for CSV deserialization that matches
/// the source CSV format's field names.
#[derive(Debug, Deserialize)]
struct CsvAirport {
    /// ICAO identifier from CSV file
    ident: String,
    /// Airport name from CSV file
    name: String,
}

/// Application state holding immutable airport data shared across all requests.
///
/// # Fields
/// - `airports`: Preloaded list of airports with search-optimized fields
pub struct AppState {
    pub airports: Vec<Airport>,
}

/// Unified error type for API operations, implementing Actix's `ResponseError`.
#[derive(Debug, Error)]
pub enum ApiError {
    /// Occurs when CSV parsing fails (malformed data or I/O issues)
    #[error("CSV parsing error: {0}")]
    CsvError(#[from] csv::Error),

    /// Occurs during file operations (e.g., missing airports.csv)
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),

    /// General catch-all for unexpected errors
    #[error("Internal server error")]
    InternalError,
}

/// Implementation of Actix's error response conversion
impl ResponseError for ApiError {
    /// Converts API errors into HTTP responses with appropriate status codes
    /// and JSON-formatted error messages.
    fn error_response(&self) -> HttpResponse {
        HttpResponse::InternalServerError().json(serde_json::json!({ "error": self.to_string() }))
    }
}

/// Query parameters for pagination controls
#[derive(Debug, Deserialize)]
pub struct PaginationParams {
    /// Maximum number of items to return (1-50, default: 50)
    pub limit: Option<usize>,
    /// Starting offset for pagination (default: 0)
    pub offset: Option<usize>,
}

/// Query parameters for search operations
#[derive(Debug, Deserialize)]
pub struct SearchParams {
    /// Search query string (case-insensitive partial matches)
    pub q: String,
    /// Maximum number of results to return (1-50, default: 50)
    pub limit: Option<usize>,
    /// Starting offset for paginated results (default: 0)
    pub offset: Option<usize>,
}

/// Handler for GET /airports endpoint returning paginated airport list
///
/// # Parameters
/// - `data`: Application state with airport list
/// - `query`: Pagination parameters from URL query string
///
/// # Response
/// - JSON-encoded PaginatedResponse containing airport data slice
#[get("/airports")]
async fn get_airports(
    data: web::Data<AppState>,
    query: web::Query<PaginationParams>,
) -> Result<HttpResponse, ApiError> {
    let response = paginate(&data.airports, query.offset, query.limit);
    Ok(HttpResponse::Ok().json(response))
}

/// Handler for GET /airports/search endpoint with parallelized filtering
///
/// # Parameters
/// - `data`: Application state with airport list
/// - `query`: Search parameters including query string and pagination
///
/// # Behavior
/// - Performs case-insensitive search on ICAO codes and names
/// - Uses Rayon's parallel iterator for efficient multi-core filtering
/// - Applies pagination to filtered results
///
/// # Response
/// - JSON-encoded PaginatedResponse containing matching airports
#[get("/airports/search")]
async fn search_airports(
    data: web::Data<AppState>,
    query: web::Query<SearchParams>,
) -> Result<HttpResponse, ApiError> {
    let search_query = query.q.to_lowercase();

    // Parallel filtering using Rayon's par_iter for multi-core performance
    let filtered: Vec<&Airport> = data
        .airports
        .par_iter()
        .filter(|airport| {
            airport.lower_icao.contains(&search_query) || airport.lower_name.contains(&search_query)
        })
        .collect();

    let response = paginate(&filtered, query.offset, query.limit);
    Ok(HttpResponse::Ok().json(response))
}

/// Loads airport data from CSV file with validation and preprocessing
///
/// # Parameters
/// - `path`: Filesystem path to CSV file
///
/// # Returns
/// - Vector of parsed Airport records
/// - Skips entries with empty ICAO codes
///
/// # Preprocessing
/// - Converts ICAO and names to lowercase for search optimization
/// - Stores original case values for display purposes
pub fn load_airports(path: &str) -> Result<Vec<Airport>, ApiError> {
    let mut rdr = csv::Reader::from_path(path)?;
    let mut airports = Vec::new();

    for result in rdr.deserialize() {
        let record: CsvAirport = result?;
        if !record.ident.trim().is_empty() {
            airports.push(Airport {
                lower_icao: record.ident.to_lowercase(),
                lower_name: record.name.to_lowercase(),
                icao: record.ident,
                name: record.name,
            });
        }
    }
    info!("Loaded {} airports", airports.len());
    Ok(airports)
}

/// Configures and starts the Actix web server
///
/// # Setup Steps
/// 1. Initialize logging
/// 2. Load airport data from CSV
/// 3. Create shared application state
/// 4. Configure HTTP server with routes and middleware
///
/// # Server Features
/// - Request logging via Actix's Logger middleware
/// - JSON error handling
/// - Shared immutable state for thread-safe data access
#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init();
    let airports = load_airports("airports.csv").expect("Failed to load airports.csv");
    let app_state = web::Data::new(AppState { airports });

    info!("Starting server at http://0.0.0.0:8080");

    HttpServer::new(move || {
        App::new()
            .wrap(Logger::default())
            .app_data(app_state.clone())
            .service(get_airports)
            .service(search_airports)
    })
    .bind("0.0.0.0:8080")?
    .run()
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{test, App};
    use serde::Deserialize;

    /// Test-specific response structure enabling deserialization
    /// of paginated responses with typed data payloads
    #[derive(Debug, Deserialize)]
    struct TestPaginatedResponse<T> {
        total: usize,
        has_more: bool,
        remaining: usize,
        data: T,
    }

    /// Creates test application state with predefined airport data
    fn create_test_state() -> web::Data<AppState> {
        let airports = vec![
            Airport {
                icao: "KJFK".into(),
                name: "John F. Kennedy International Airport".into(),
                lower_icao: "kjfk".into(),
                lower_name: "john f. kennedy international airport".into(),
            },
            Airport {
                icao: "KLAX".into(),
                name: "Los Angeles International Airport".into(),
                lower_icao: "klax".into(),
                lower_name: "los angeles international airport".into(),
            },
            Airport {
                icao: "EGLL".into(),
                name: "London Heathrow Airport".into(),
                lower_icao: "egll".into(),
                lower_name: "london heathrow airport".into(),
            },
        ];
        web::Data::new(AppState { airports })
    }

    /// Tests basic airport listing without pagination parameters
    #[actix_web::test]
    async fn test_get_airports_no_pagination() {
        let state = create_test_state();
        let app =
            test::init_service(App::new().app_data(state.clone()).service(get_airports)).await;
        let req = test::TestRequest::get().uri("/airports").to_request();
        let resp: TestPaginatedResponse<Vec<Airport>> =
            test::call_and_read_body_json(&app, req).await;
        assert_eq!(resp.total, 3);
        assert_eq!(resp.data.len(), 3);
        assert!(!resp.has_more);
        assert_eq!(resp.remaining, 0);
    }

    /// Tests pagination behavior with offset and limit parameters
    #[actix_web::test]
    async fn test_get_airports_with_pagination() {
        let state = create_test_state();
        let app =
            test::init_service(App::new().app_data(state.clone()).service(get_airports)).await;
        let req = test::TestRequest::get()
            .uri("/airports?limit=2&offset=1")
            .to_request();
        let resp: TestPaginatedResponse<Vec<Airport>> =
            test::call_and_read_body_json(&app, req).await;
        assert_eq!(resp.total, 3);
        assert_eq!(resp.data.len(), 2);
        assert_eq!(resp.data[0].icao, "KLAX");
        assert!(!resp.has_more);
        assert_eq!(resp.remaining, 0);
    }

    /// Tests successful search operation with exact ICAO match
    #[actix_web::test]
    async fn test_search_airports() {
        let state = create_test_state();
        let app =
            test::init_service(App::new().app_data(state.clone()).service(search_airports)).await;
        let req = test::TestRequest::get()
            .uri("/airports/search?q=kjfk")
            .to_request();
        let resp: TestPaginatedResponse<Vec<Airport>> =
            test::call_and_read_body_json(&app, req).await;
        assert_eq!(resp.total, 1);
        assert_eq!(resp.data.len(), 1);
        assert_eq!(resp.data[0].icao, "KJFK");
        assert!(!resp.has_more);
        assert_eq!(resp.remaining, 0);
    }

    /// Tests search behavior with non-matching query
    #[actix_web::test]
    async fn test_search_airports_no_match() {
        let state = create_test_state();
        let app =
            test::init_service(App::new().app_data(state.clone()).service(search_airports)).await;
        let req = test::TestRequest::get()
            .uri("/airports/search?q=XYZ")
            .to_request();
        let resp: TestPaginatedResponse<Vec<Airport>> =
            test::call_and_read_body_json(&app, req).await;
        assert_eq!(resp.total, 0);
        assert_eq!(resp.data.len(), 0);
        assert!(!resp.has_more);
        assert_eq!(resp.remaining, 0);
    }
}
