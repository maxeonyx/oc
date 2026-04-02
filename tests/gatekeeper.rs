#[test]
fn tdd_ratchet_gatekeeper() {
    if std::env::var("TDD_RATCHET").is_err() {
        panic!(
            "\n\nThis project uses strict TDD via tdd-ratchet.\nDo not run `cargo test` directly.\nRun `cargo ratchet` instead.\n"
        );
    }
}
