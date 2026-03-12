#!/bin/bash
set -euo pipefail

# ═══════════════════════════════════════════════════════════════════════
# Ordo Scientific Benchmark Runner v1.0
# Per benchmark-spec.md — strict resource isolation, warmup/cooldown,
# 5-round repetition with statistical analysis.
# ═══════════════════════════════════════════════════════════════════════

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

# ─── Config ─────────────────────────────────────────────────────────
DURATION=30           # seconds per measurement
WARMUP_N1=1000        # warmup phase 1 requests
WARMUP_N2=5000        # warmup phase 2 requests
COOLDOWN_SAME=15      # seconds between same-engine tests
COOLDOWN_SWITCH=30    # seconds between different engines
RUNS=5                # repetitions per test point
PORT=8080             # all engines bind to this port inside container

# ─── Parse arguments ───────────────────────────────────────────────
# Usage: benchmark-runner.sh [a|b|c|all] [--results-dir DIR] [--engine ENGINE]
LAYER="all"
RESULTS_DIR=""
ENGINE_FILTER=""

while [ $# -gt 0 ]; do
    case "$1" in
        a|b|c|all)      LAYER="$1" ;;
        --results-dir)  RESULTS_DIR="$2"; shift ;;
        --engine)       ENGINE_FILTER="$2"; shift ;;
        -h|--help)
            echo "Usage: $0 [a|b|c|all] [--results-dir DIR] [--engine ENGINE]"
            echo "  --results-dir  Append to existing results directory (for re-runs)"
            echo "  --engine       Only run specified engine (ordo|opa|json-rules|grule)"
            exit 0 ;;
        *) echo "Unknown arg: $1"; exit 1 ;;
    esac
    shift
done

[ -z "$RESULTS_DIR" ] && RESULTS_DIR="$SCRIPT_DIR/results/$(date +%Y%m%d_%H%M%S)"
RAW_DIR="$RESULTS_DIR/raw"

# ─── Test inputs ────────────────────────────────────────────────────
INPUT_L1='{"score":75}'
INPUT_L2='{"score":75}'
INPUT_L3='{"membership":"gold","amount":7500,"region":"domestic"}'
# Ordo uses nested JSON
INPUT_L3_ORDO='{"input":{"user":{"membership":"gold","region":"domestic"},"order":{"amount":7500}}}'
INPUT_L4='{"txn_amount":75000,"risk_score":65,"txn_count":30,"trust_level":6}'
INPUT_L4_ORDO='{"input":{"transaction":{"amount":75000},"user":{"profile":{"risk_score":65},"history":{"transactions":[1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30]}},"device":{"trust_level":6}}}'

# OPA wraps in input.xxx
INPUT_L1_OPA='{"input":{"score":75}}'
INPUT_L2_OPA='{"input":{"score":75}}'
INPUT_L3_OPA='{"input":{"user":{"membership":"gold","region":"domestic"},"order":{"amount":7500}}}'
INPUT_L4_OPA='{"input":{"transaction":{"amount":75000},"user":{"profile":{"risk_score":65},"history":{"transactions":[1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30]}},"device":{"trust_level":6}}}'

# ─── Functions ──────────────────────────────────────────────────────

log() { echo "[$(date +%H:%M:%S)] $*"; }

preflight() {
    log "=== Preflight checks ==="
    command -v docker >/dev/null || { echo "ERROR: docker not found"; exit 1; }
    command -v hey >/dev/null    || { echo "ERROR: hey not found"; exit 1; }
    command -v jq >/dev/null     || { echo "ERROR: jq not found"; exit 1; }

    # Check Docker memory — warn if < 10GB
    local mem_bytes=$(docker system info --format '{{.MemTotal}}' 2>/dev/null || echo 0)
    local mem_gb=$((mem_bytes / 1073741824))
    if [ "$mem_gb" -lt 10 ]; then
        log "WARNING: Docker has ${mem_gb}GB RAM. Recommend ≥10GB for accurate benchmarks."
        log "         Go to Docker Desktop → Settings → Resources → Memory"
    fi

    mkdir -p "$RAW_DIR"/{layer-a,layer-b,layer-c}
    mkdir -p "$RESULTS_DIR"/{aggregated,correctness}
    log "Results dir: $RESULTS_DIR"
}

collect_system_info() {
    log "Collecting system info..."
    cat > "$RESULTS_DIR/metadata.json" <<SYSINFOEOF
{
    "timestamp": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
    "hostname": "$(hostname)",
    "os": "$(uname -s) $(uname -r)",
    "arch": "$(uname -m)",
    "cpu": "$(sysctl -n machdep.cpu.brand_string 2>/dev/null || echo unknown)",
    "ncpu": $(sysctl -n hw.ncpu 2>/dev/null || echo 0),
    "memory_gb": $(($(sysctl -n hw.memsize 2>/dev/null || echo 0) / 1073741824)),
    "docker_version": "$(docker version --format '{{.Server.Version}}' 2>/dev/null || echo unknown)",
    "docker_cpus": $(docker system info --format '{{.NCPU}}' 2>/dev/null || echo 0),
    "docker_mem_gb": $(($(docker system info --format '{{.MemTotal}}' 2>/dev/null || echo 0) / 1073741824)),
    "hey_version": "$(hey 2>&1 | head -1 || echo unknown)",
    "rust_version": "$(rustc --version 2>/dev/null || echo unknown)",
    "ordo_commit": "$(cd "$PROJECT_ROOT" && git rev-parse HEAD 2>/dev/null || echo unknown)"
}
SYSINFOEOF
    log "System info saved."
}

build_images() {
    log "=== Building Docker images ==="

    log "Building ordo-bench..."
    docker build -t ordo-bench \
        -f "$SCRIPT_DIR/docker/Dockerfile.ordo" \
        "$PROJECT_ROOT" 2>&1 | tail -3

    log "Building opa-bench..."
    docker build -t opa-bench \
        -f "$SCRIPT_DIR/docker/Dockerfile.opa" \
        "$SCRIPT_DIR" 2>&1 | tail -3

    log "Building json-rules-bench..."
    cd "$SCRIPT_DIR/rules/json-rules" && npm install --silent 2>/dev/null
    docker build -t json-rules-bench \
        -f "$SCRIPT_DIR/docker/Dockerfile.json-rules" \
        "$SCRIPT_DIR" 2>&1 | tail -3

    log "Building grule-bench..."
    docker build -t grule-bench \
        -f "$SCRIPT_DIR/docker/Dockerfile.grule" \
        "$SCRIPT_DIR" 2>&1 | tail -3

    log "All images built."
}

start_engine() {
    local name="$1" image="$2" cpus="$3" mem="$4"
    shift 4
    local extra_args="$*"

    docker rm -f bench-target 2>/dev/null || true
    sleep 1

    log "Starting $name (cpus=$cpus, mem=$mem)..."
    if [ "$cpus" = "all" ]; then
        docker run -d --name bench-target --memory="$mem" -p $PORT:8080 $extra_args "$image"
    else
        docker run -d --name bench-target --cpuset-cpus="$cpus" --memory="$mem" -p $PORT:8080 $extra_args "$image"
    fi

    # Wait for health
    local retries=30
    while [ $retries -gt 0 ]; do
        if curl -sf http://localhost:$PORT/health >/dev/null 2>&1; then
            log "$name is ready."
            return 0
        fi
        sleep 1
        retries=$((retries - 1))
    done
    log "ERROR: $name failed to start"
    docker logs bench-target 2>&1 | tail -20
    return 1
}

stop_engine() {
    docker stop bench-target 2>/dev/null || true
    docker rm -f bench-target 2>/dev/null || true
}

warmup() {
    local url="$1" body="$2" conc="$3"
    log "  Warmup: W1 (1000 reqs, c=1)..."
    hey -n $WARMUP_N1 -c 1 -m POST -H "Content-Type: application/json" -d "$body" "$url" >/dev/null 2>&1
    log "  Warmup: W2 (5000 reqs, c=$conc)..."
    hey -n $WARMUP_N2 -c "$conc" -m POST -H "Content-Type: application/json" -d "$body" "$url" >/dev/null 2>&1
    log "  Warmup: W3 (sleep 5s)..."
    sleep 5
}

measure() {
    local url="$1" body="$2" conc="$3" outfile="$4"
    log "  Measuring: ${DURATION}s, c=$conc → $outfile"
    # Single run — capture summary text (hey writes summary to stdout when no -o)
    hey -z "${DURATION}s" -c "$conc" -m POST \
        -H "Content-Type: application/json" \
        -d "$body" "$url" > "${outfile%.csv}.txt" 2>/dev/null
}

collect_docker_stats() {
    local outfile="$1"
    docker stats bench-target --no-stream --format '{{.CPUPerc}},{{.MemUsage}}' > "$outfile" 2>/dev/null
}

cooldown() {
    local seconds="$1"
    log "  Cooldown: ${seconds}s..."
    sleep "$seconds"
}

parse_summary() {
    local txtfile="$1"
    local qps avg p50 p95 p99 max_lat
    qps=$(grep "Requests/sec:" "$txtfile" | awk '{printf "%.0f", $2}')
    avg=$(grep "Average:" "$txtfile" | head -1 | awk '{printf "%.3f", $2*1000}')
    p50=$(grep "50% in" "$txtfile" | awk '{printf "%.3f", $3*1000}')
    p95=$(grep "95% in" "$txtfile" | awk '{printf "%.3f", $3*1000}')
    p99=$(grep "99% in" "$txtfile" | awk '{printf "%.3f", $3*1000}')
    max_lat=$(grep "Slowest:" "$txtfile" | awk '{printf "%.3f", $2*1000}')
    echo "${qps:-0},${avg:-0},${p50:-0},${p95:-0},${p99:-0},${max_lat:-0}"
}

# ─── Engine-specific URL/input helpers ──────────────────────────────

get_url() {
    local engine="$1" level="$2"
    case "$engine" in
        ordo)       echo "http://localhost:$PORT/api/v1/execute/$level" ;;
        opa)        echo "http://localhost:$PORT/v1/data/$level/result" ;;
        json-rules) echo "http://localhost:$PORT/execute/$level" ;;
        grule)      echo "http://localhost:$PORT/execute/$level" ;;
    esac
}

get_input() {
    local engine="$1" level="$2"
    case "${engine}_${level}" in
        ordo_L1-trivial) echo '{"input":{"score":75}}' ;;
        ordo_L2-simple)  echo '{"input":{"score":75}}' ;;
        ordo_L3-medium)  echo "$INPUT_L3_ORDO" ;;
        ordo_L4-complex) echo "$INPUT_L4_ORDO" ;;
        opa_L1)  echo "$INPUT_L1_OPA" ;;
        opa_L2)  echo "$INPUT_L2_OPA" ;;
        opa_L3)  echo "$INPUT_L3_OPA" ;;
        opa_L4)  echo "$INPUT_L4_OPA" ;;
        json-rules_L1) echo "$INPUT_L1" ;;
        json-rules_L2) echo "$INPUT_L2" ;;
        json-rules_L3) echo "$INPUT_L3" ;;
        json-rules_L4) echo "$INPUT_L4" ;;
        grule_L1) echo "$INPUT_L1" ;;
        grule_L2) echo "$INPUT_L2" ;;
        grule_L3) echo "$INPUT_L3" ;;
        grule_L4) echo "$INPUT_L4" ;;
    esac
}

get_image() {
    case "$1" in
        ordo)       echo "ordo-bench" ;;
        opa)        echo "opa-bench" ;;
        json-rules) echo "json-rules-bench" ;;
        grule)      echo "grule-bench" ;;
    esac
}

get_level_name() {
    local engine="$1" level="$2"
    case "$engine" in
        ordo) echo "${level}-${LEVEL_NAMES[$level]}" ;;  # L1-trivial, etc.
        *)    echo "$level" ;;
    esac
}

# Ordo needs rules created via API
setup_ordo_rules() {
    local rules_json="$SCRIPT_DIR/rules/ordo/rules.json"
    local level
    for level in L1 L2 L3 L4; do
        local rule_name
        case $level in
            L1) rule_name="L1-trivial" ;;
            L2) rule_name="L2-simple" ;;
            L3) rule_name="L3-medium" ;;
            L4) rule_name="L4-complex" ;;
        esac
        local payload=$(jq -r ".$level" "$rules_json")
        curl -sf -X POST "http://localhost:$PORT/api/v1/rulesets" \
            -H "Content-Type: application/json" \
            -d "$payload" >/dev/null 2>&1 || log "  Warning: Failed to create rule $rule_name"
    done
    log "  Ordo rules created."
}

# ─── Core test function ─────────────────────────────────────────────

run_test_point() {
    local engine="$1" level="$2" conc="$3" cpus="$4" mem="$5" layer="$6"
    local image=$(get_image "$engine")
    local url=$(get_url "$engine" "$level")

    local ordo_level="$level"
    case "$engine" in
        ordo)
            case $level in
                L1) ordo_level="L1-trivial" ;;
                L2) ordo_level="L2-simple" ;;
                L3) ordo_level="L3-medium" ;;
                L4) ordo_level="L4-complex" ;;
            esac
            url="http://localhost:$PORT/api/v1/execute/$ordo_level"
            ;;
    esac

    local input=$(get_input "$engine" "$ordo_level")
    local cpu_label="${cpus//,/-}"
    [ "$cpus" = "all" ] && cpu_label="all"

    log "── Test: $engine $level c=$conc cores=$cpu_label ($layer) ──"

    # Start engine
    start_engine "$engine" "$image" "$cpus" "$mem"

    # Ordo needs rules created via API
    if [ "$engine" = "ordo" ]; then
        sleep 2
        setup_ordo_rules
    fi

    # Run 5 rounds
    for run in $(seq 1 $RUNS); do
        local base="${engine}_${level}_c${conc}_${cpu_label}_run${run}"

        warmup "$url" "$input" "$conc"

        # Collect docker stats during measurement (background)
        (for i in $(seq 1 6); do
            sleep 5
            collect_docker_stats "$RAW_DIR/$layer/${base}_stats_${i}.txt"
        done) &
        local stats_pid=$!

        measure "$url" "$input" "$conc" "$RAW_DIR/$layer/${base}.csv"

        kill $stats_pid 2>/dev/null || true
        wait $stats_pid 2>/dev/null || true

        # Parse and log
        local summary=$(parse_summary "$RAW_DIR/$layer/${base}.txt")
        local qps=$(echo "$summary" | cut -d, -f1)
        log "  Run $run/$RUNS: QPS=$qps"

        cooldown $COOLDOWN_SAME
    done

    stop_engine
    cooldown $COOLDOWN_SWITCH
}

# ─── Aggregate results ──────────────────────────────────────────────

aggregate_layer() {
    local layer="$1"
    local outfile="$RESULTS_DIR/aggregated/${layer}-summary.csv"
    echo "engine,level,concurrency,cores,run,qps,avg_ms,p50_ms,p95_ms,p99_ms,max_ms" > "$outfile"

    for txtfile in "$RAW_DIR/$layer"/*.txt; do
        [ -f "$txtfile" ] || continue
        [[ "$txtfile" == *_stats_* ]] && continue

        local base=$(basename "$txtfile" .txt)
        # Parse: engine_level_cN_cores_runN
        local engine=$(echo "$base" | cut -d_ -f1)
        local level=$(echo "$base" | cut -d_ -f2)
        local conc=$(echo "$base" | cut -d_ -f3 | sed 's/^c//')
        local cores=$(echo "$base" | cut -d_ -f4)
        local run=$(echo "$base" | cut -d_ -f5 | sed 's/^run//')

        local summary=$(parse_summary "$txtfile")
        echo "$engine,$level,$conc,$cores,$run,$summary" >> "$outfile"
    done

    log "Aggregated: $outfile"
}

# ─── Layer execution ────────────────────────────────────────────────

run_layer_a() {
    log "═══ Layer A: Cross-Engine Comparison (4-core) ═══"
    local engines=(ordo opa json-rules grule)
    local levels=(L1 L2 L3 L4)
    local concs=(1 50 200)

    for engine in "${engines[@]}"; do
        [ -n "$ENGINE_FILTER" ] && [ "$engine" != "$ENGINE_FILTER" ] && continue
        for level in "${levels[@]}"; do
            for conc in "${concs[@]}"; do
                run_test_point "$engine" "$level" "$conc" "0-3" "8g" "layer-a"
            done
        done
    done

    aggregate_layer "layer-a"
}

run_layer_b() {
    log "═══ Layer B: Core Scaling (Ordo + OPA) ═══"
    local engines=(ordo opa)
    local levels=(L2 L3)
    local concs=(50 100)
    local core_configs=("0" "0-1" "0-3" "all")
    local mem_configs=("2g" "4g" "8g" "14g")

    for engine in "${engines[@]}"; do
        [ -n "$ENGINE_FILTER" ] && [ "$engine" != "$ENGINE_FILTER" ] && continue
        for level in "${levels[@]}"; do
            for conc in "${concs[@]}"; do
                for i in "${!core_configs[@]}"; do
                    run_test_point "$engine" "$level" "$conc" "${core_configs[$i]}" "${mem_configs[$i]}" "layer-b"
                done
            done
        done
    done

    aggregate_layer "layer-b"
}

run_layer_c() {
    log "═══ Layer C: Ordo Deep Profile (4-core) ═══"
    local levels=(L1 L2 L3 L4)
    local concs=(1 10 50 100 200)

    for level in "${levels[@]}"; do
        for conc in "${concs[@]}"; do
            run_test_point "ordo" "$level" "$conc" "0-3" "8g" "layer-c"
        done
    done

    aggregate_layer "layer-c"
}

# ─── Main ───────────────────────────────────────────────────────────

main() {
    preflight
    collect_system_info
    build_images

    case "$LAYER" in
        a)   run_layer_a ;;
        b)   run_layer_b ;;
        c)   run_layer_c ;;
        all)
            run_layer_a
            run_layer_b
            run_layer_c
            ;;
        *)
            echo "Usage: $0 [a|b|c|all]"
            exit 1
            ;;
    esac

    log "═══ All tests complete! ═══"
    log "Results: $RESULTS_DIR"
    log "Aggregated CSVs in: $RESULTS_DIR/aggregated/"
}

main "$@"
