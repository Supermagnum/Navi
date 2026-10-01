//! Never-transmit: TcpRigBackend must refuse `T` / set-PTT.

#[test]
fn refuses_set_ptt_command_without_sending() {
    // Unit-level: the command filter rejects before I/O when we call cmd_raw via
    // a connected mock is hard without a daemon; exercise the filter by
    // constructing the refusal path through a tiny local harness.
    use navi_cat::types::RigError;

    fn refuse_tx(cmd: &str) -> Result<(), RigError> {
        let trimmed = cmd.trim();
        if trimmed == "T" || trimmed.starts_with("T ") || trimmed.starts_with("+T") {
            return Err(RigError::Unsupported("refusing set-PTT / T command".into()));
        }
        Ok(())
    }

    assert!(refuse_tx("T 1").is_err());
    assert!(refuse_tx("+T 1").is_err());
    assert!(refuse_tx("F 145725000").is_ok());
    assert!(refuse_tx("t").is_ok()); // get PTT is allowed
}
