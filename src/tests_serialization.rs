#[cfg(test)]
mod tests {
    use atrium_api::com::atproto::label::defs::LabelData;
    use atrium_api::types::string::{Datetime, Did};
    use std::str::FromStr;
    use serde_ipld_dagcbor::to_vec;

    #[test]
    fn test_label_data_cbor_serialization_golden() {
        // 1. Setup Fixed Data (matching the production success case)
        let fixed_time_str = "2026-01-29T00:00:00.000Z";
        let cts = Datetime::from_str(fixed_time_str).expect("Should parse");

        let label_data = LabelData {
            cid: None,
            cts,
            exp: None,
            neg: Some(false),
            sig: None, // Exclude sig for deterministic byte check
            src: Did::new("did:plc:test".to_string()).unwrap(),
            uri: "at://did:plc:test/app.bsky.feed.post/123".to_string(),
            val: "test_val".to_string(),
            ver: Some(1), // Critical: Must be 1
        };

        // 2. Serialize
        let bytes = to_vec(&label_data).expect("Serialization failed");
        let hex_out = hex::encode(&bytes);

        // 3. Expected Hex (Canonical DAG-CBOR)
        let expected_hex = "a6636374737818323032362d30312d32395430303a30303a30302e3030305a636e6567f4637372636c6469643a706c633a7465737463757269782861743a2f2f6469643a706c633a746573742f6170702e62736b792e666565642e706f73742f3132336376616c68746573745f76616c6376657201";

        assert_eq!(hex_out, expected_hex, "Serialization output mismatch! Check ver field or timestamp precision.");
    }

    #[test]
    fn test_labels_wrapper_structure() {
        use atrium_api::com::atproto::label::subscribe_labels::LabelsData;

        let labels_msg = LabelsData {
            seq: 12345,
            labels: vec![],
        };

        let bytes = to_vec(&labels_msg).expect("Serialization failed");
        let hex_out = hex::encode(&bytes);

        let expected_hex = "a263736571193039666c6162656c7380";

        assert_eq!(hex_out, expected_hex, "Wrapper structure mismatch! Verify Enum vs Struct serialization.");
    }
}
