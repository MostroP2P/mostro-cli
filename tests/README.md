# Mostro CLI Test Suite

This directory contains comprehensive unit and integration tests for the Mostro CLI application.

## Test Files

### Core Test Files

1. **`parser_dms.rs`** (16 tests)
   - Direct message parsing and display
   - Message payload handling
   - Mostro identification
   - Edge cases and error handling

2. **`cli_functions.rs`** (26 tests)
   - CLI command logic
   - Message creation and serialization
   - Payload validation
   - Action handling

3. **`util_misc.rs`** (13 tests)
   - Utility function tests
   - Path check
   - String manipulation

4. **`parser_orders.rs`** (11 tests)
   - Order event parsing
   - Filter validation
   - Table display formatting

5. **`parser_disputes.rs`** (9 tests)
   - Dispute event parsing
   - Status handling
   - Display formatting

6. **`integration_tests.rs`** (3 tests)
   - Context creation
   - Integration scenarios

## Running Tests

### Run all tests
```bash
cargo test
```

### Run tests with output
```bash
cargo test -- --nocapture
```

### Run specific test file
```bash
cargo test --test parser_dms
cargo test --test cli_functions
cargo test --test util_misc
```

### Run a specific test
```bash
cargo test test_orders_info_empty_order_ids
```

### Run tests in parallel (default)
```bash
cargo test -- --test-threads=4
```

### Run tests serially
```bash
cargo test -- --test-threads=1
```

## Test Coverage

**Total Tests:** 78
- Unit Tests: 75 (97%)
- Integration Tests: 3 (3%)
- Async Tests: 16 (21%)
- Sync Tests: 62 (79%)

## Key Areas Tested

### 1. New Features
- ✅ `orders_info` command (5 tests)
- ✅ Enhanced `restore` command with response handling (2 tests)
- ✅ Table-based message display (16 tests)
- ✅ Colored output and icons (covered in display tests)

### 2. Modified Features
- ✅ Enhanced dispute handling (9 tests)
- ✅ Improved order display (11 tests)
- ✅ Rating system validation (3 tests)

### 3. Edge Cases
- ✅ Empty collections
- ✅ Invalid inputs
- ✅ Boundary conditions
- ✅ Data integrity

## Test Patterns

### Message Creation
```rust
let message = Message::new_order(
    Some(order_id),
    Some(request_id),
    Some(trade_index),
    Action::Orders,
    Some(payload),
);
```

### Async Testing
```rust
#[tokio::test]
async fn test_name() {
    let result = async_function().await;
    assert!(result.is_ok());
}
```

### Payload Validation
```rust
match payload {
    Payload::Expected(data) => {
        assert_eq!(data, expected);
    }
    _ => panic!("Unexpected payload type"),
}
```

## Best Practices

1. **Descriptive Names** - Test names clearly describe what is being tested
2. **AAA Pattern** - Arrange, Act, Assert structure
3. **Independence** - Tests don't depend on each other
4. **Fast Execution** - No network calls or heavy I/O
5. **Deterministic** - Consistent results across runs

## Contributing

When adding new tests:

1. Follow existing naming conventions
2. Use appropriate test attributes (`#[test]` or `#[tokio::test]`)
3. Test happy paths, edge cases, and error conditions
4. Keep tests focused and simple
5. Add documentation for complex test logic

## CI/CD

These tests are automatically run in CI/CD pipelines. All tests must pass before code can be merged.

## Documentation

For detailed test documentation, see [`TEST_SUMMARY.md`](../TEST_SUMMARY.md) in the repository root.