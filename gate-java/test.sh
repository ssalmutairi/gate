#!/bin/bash
# Gate Java API Gateway — Test Script
# Usage: ./test.sh [host:port]

BASE="${1:-http://localhost:8080}"
PASS=0
FAIL=0

run() {
    local cmd="$1"
    echo "  > $cmd" >&2
    eval "$cmd"
}

check() {
    local desc="$1" expected="$2" actual="$3"
    if echo "$actual" | grep -q "$expected"; then
        echo "  ✅ $desc"
        ((PASS++))
    else
        echo "  ❌ $desc (expected: $expected)"
        ((FAIL++))
    fi
    echo ""
}

echo "=== Gate Java Test Suite ==="
echo "Target: $BASE"
echo ""

# 1. Health
echo "--- Health ---"
CMD="curl -s $BASE/health"
resp=$(run "$CMD")
check "GET /health returns ok" '"status":"ok"' "$resp"

# 2. Services
echo "--- Services ---"
CMD="curl -s $BASE/services"
resp=$(run "$CMD")
check "GET /services returns list" '"total"' "$resp"

# 3. JSON API (dummyjson)
echo "--- JSON Proxy (dummyjson) ---"
CMD="curl -s -o /dev/null -w %{http_code} $BASE/dummyjson/products/1"
resp=$(run "$CMD")
check "GET /dummyjson/products/1 -> 200" "200" "$resp"

CMD="curl -s $BASE/dummyjson/products/1"
resp=$(run "$CMD")
check "Response has product title" '"title"' "$resp"

CMD="curl -s -o /dev/null -w %{http_code} $BASE/dummyjson/carts"
resp=$(run "$CMD")
check "GET /dummyjson/carts -> 200" "200" "$resp"

# 4. SOAP (calculator)
echo "--- SOAP Proxy (calculator) ---"
CMD='curl -s -X POST '"$BASE"'/calculator/calculator.asmx -H "Content-Type: text/xml; charset=utf-8" -H "SOAPAction: \"http://tempuri.org/Add\"" -d '"'"'<?xml version="1.0" encoding="utf-8"?><soap:Envelope xmlns:soap="http://schemas.xmlsoap.org/soap/envelope/" xmlns:tem="http://tempuri.org/"><soap:Body><tem:Add><tem:intA>10</tem:intA><tem:intB>20</tem:intB></tem:Add></soap:Body></soap:Envelope>'"'"
resp=$(run "$CMD")
check "SOAP Add 10+20=30" "<AddResult>30</AddResult>" "$resp"

CMD='curl -s -X POST '"$BASE"'/calculator/calculator.asmx -H "Content-Type: text/xml; charset=utf-8" -H "SOAPAction: \"http://tempuri.org/Multiply\"" -d '"'"'<?xml version="1.0" encoding="utf-8"?><soap:Envelope xmlns:soap="http://schemas.xmlsoap.org/soap/envelope/" xmlns:tem="http://tempuri.org/"><soap:Body><tem:Multiply><tem:intA>7</tem:intA><tem:intB>6</tem:intB></tem:Multiply></soap:Body></soap:Envelope>'"'"
resp=$(run "$CMD")
check "SOAP Multiply 7*6=42" "<MultiplyResult>42</MultiplyResult>" "$resp"

# 5. Unknown service -> 404
echo "--- Error Handling ---"
CMD="curl -s -o /dev/null -w %{http_code} $BASE/unknown/test"
resp=$(run "$CMD")
check "GET /unknown/test -> 404" "404" "$resp"

# Results
echo "=== Results: $PASS passed, $FAIL failed ==="
exit $FAIL
