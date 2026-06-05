build_demo_kagi() {
  cargo build --quiet
  KAGI_DEMO_BIN="$PWD/target/debug/kagi"
  export KAGI_DEMO_BIN
}
