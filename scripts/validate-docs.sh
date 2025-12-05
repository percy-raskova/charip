#!/usr/bin/env bash
# Documentation validation script for charip-lsp
# Simulates CI checks locally before pushing

set -e

echo "=== Documentation Validation ==="
echo ""

echo "1. Building docs with warnings as errors..."
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --lib
echo "   [PASS] Documentation builds without warnings"
echo ""

echo "2. Verifying redirect target exists..."
if [ -d target/doc/charip ]; then
    echo "   [PASS] target/doc/charip/ exists"
else
    echo "   [FAIL] target/doc/charip/ not found"
    exit 1
fi
echo ""

echo "3. Running doc tests..."
cargo test --doc
echo "   [PASS] Doc tests passed"
echo ""

echo "4. Running integration tests..."
cargo test --tests
echo "   [PASS] Integration tests passed"
echo ""

echo "5. Running library tests..."
cargo test --lib
echo "   [PASS] Library tests passed"
echo ""

echo "=== All checks passed! ==="
