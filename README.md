# ICAO API

A Rust-based API for fetching ICAO airport codes.

## Features

- **Pagination Support**: Efficient slice-based pagination with configurable limits
- **Parallel Search**: Multi-core optimized search using Rayon parallel iterators
- **Zero-Copy Operations**: Memory-efficient handling of large datasets through slice references
- **Case-Insensitive Matching**: Precomputed lowercase fields for fast search operations
- **CSV Data Loading**: Load airport data from properly formatted CSV files
- **Production-Ready**: Built-in logging, error handling, and configurable limits
- **Test Coverage**: Comprehensive test suite including pagination and search scenarios

## Installation

### Prerequisites

- Rust (install via [rustup](https://rustup.rs/))
- Cargo (Rust's package manager)

### Steps

1. Clone the repository:
   ```bash
   git clone https://git.vsulimov.com/icao-api.git
   cd icao-api
   ```

2. Add an `airports.csv` file in the project root with the following structure:
   ```csv
   ident,name
   KJFK,John F. Kennedy International Airport
   KLAX,Los Angeles International Airport
   EGLL,London Heathrow Airport
   ```
   You can download airports.csv from
   the [OurAirports Data](https://davidmegginson.github.io/ourairports-data/airports.csv) service.
    ```bash
   curl -O https://davidmegginson.github.io/ourairports-data/airports.csv
    ```

3. Build and run:
   ```bash
   cargo run --release
   ```

The server will start at `http://localhost:8080`.

## API Reference

### GET /airports

List airports with pagination controls

**Query Parameters**:

- `offset`: Starting index (default: 0)
- `limit`: Maximum results per page (1-50, default: 50)

**Response**:

```json
{
  "total": 3,
  "has_more": false,
  "remaining": 0,
  "data": [
    {
      "icao": "KJFK",
      "name": "John F. Kennedy International Airport"
    },
    // ... additional airports
  ]
}
```

### GET /airports/search

Search airports by ICAO code or name

**Query Parameters**:

- `q`: Search query (case-insensitive partial match)
- `offset`: Starting index (default: 0)
- `limit`: Maximum results per page (1-50, default: 50)

**Response**:
Same structure as `/airports` endpoint with filtered results

## Example Usage

### Basic Listing

```bash
curl "http://localhost:8080/airports?limit=2"
```

### Paginated Results

```bash
curl "http://localhost:8080/airports?offset=10&limit=20"
```

### Search Operation

```bash
curl "http://localhost:8080/airports/search?q=international&limit=5"
```

## Configuration

| Aspect         | Default        | Description                      |
|----------------|----------------|----------------------------------|
| Server Address | `0.0.0.0:8080` | Change in `main()` function      |
| CSV File Path  | `airports.csv` | Modify in `load_airports()` call |
| Max Page Size  | 50             | Adjust `MAX_PAGE_LIMIT` constant |

## Performance Characteristics

- **Parallel Filtering**: Utilizes all available CPU cores for search operations
- **Zero-Copy Pagination**: Avoids data duplication through slice operations
- **Precomputed Lowercase**: Eliminates runtime case conversion overhead
- **Efficient Memory Use**: Shared immutable state across request handlers

## Error Handling

The API returns JSON-formatted errors with appropriate HTTP status codes:

```json
{
  "error": "CSV parsing error: ..."
}
```

**Common Error Types**:

- `400 Bad Request`: Invalid query parameters
- `500 Internal Server Error`: Data loading issues or unexpected failures

## Testing

Run the test suite with:

```bash
cargo test
```

Test coverage includes:

- Pagination boundary conditions
- Exact and partial match searches
- Error scenarios for missing data
- Parameter validation checks

---

**Note**: Ensure your CSV file contains at minimum `ident` and `name` columns. The system automatically creates
search-optimized lowercase versions of these fields during loading.