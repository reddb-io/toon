//! Parity port of `packages/toon-rpc/test/framing.test.mjs`: the normative
//! length-prefixed stream framing profile from spec section 8.1.

use reddb_io_toon_rpc::{encode_frame, FrameDecoder};

fn text(document: &[u8]) -> &str {
    std::str::from_utf8(document).expect("document must be UTF-8")
}

#[test]
fn a_frame_round_trips_one_complete_document() {
    let document = b"toonrpc: \"1.0\"\nresult: 2\nid: 1";
    let mut decoder = FrameDecoder::new();
    let documents = decoder.push(&encode_frame(document)).expect("valid frame");
    assert_eq!(documents.len(), 1);
    assert_eq!(text(&documents[0]), text(document));
    assert!(!decoder.has_partial_frame());
}

#[test]
fn documents_with_blank_lines_and_length_like_text_survive_framing() {
    let tricky = "first\n\n\n12\nsecond\n\n";
    let mut decoder = FrameDecoder::new();
    let documents = decoder
        .push(&encode_frame(tricky.as_bytes()))
        .expect("valid frame");
    assert_eq!(documents.len(), 1);
    assert_eq!(text(&documents[0]), tricky);
}

#[test]
fn an_empty_document_is_a_valid_frame() {
    let mut decoder = FrameDecoder::new();
    let documents = decoder.push(&encode_frame(b"")).expect("valid frame");
    assert_eq!(documents.len(), 1);
    assert!(documents[0].is_empty());
}

#[test]
fn a_frame_split_across_arbitrary_chunks_reassembles() {
    let frame = encode_frame(b"a: 1\nb: 2");
    for split in 1..frame.len() {
        let mut decoder = FrameDecoder::new();
        assert!(
            decoder
                .push(&frame[..split])
                .expect("partial frame")
                .is_empty(),
            "split at {split}"
        );
        let documents = decoder.push(&frame[split..]).expect("valid frame");
        assert_eq!(documents.len(), 1, "split at {split}");
        assert_eq!(text(&documents[0]), "a: 1\nb: 2");
    }
}

#[test]
fn multiple_frames_in_one_chunk_decode_in_order() {
    let mut chunk = encode_frame(b"one");
    chunk.extend_from_slice(&encode_frame(b"two"));
    chunk.extend_from_slice(&encode_frame(b"three"));

    let mut decoder = FrameDecoder::new();
    let documents = decoder.push(&chunk).expect("valid frames");
    let decoded = documents.iter().map(|d| text(d)).collect::<Vec<_>>();
    assert_eq!(decoded, ["one", "two", "three"]);
}

#[test]
fn byte_by_byte_delivery_of_several_frames_works() {
    let mut stream = encode_frame(b"x");
    stream.extend_from_slice(&encode_frame(b"yz"));

    let mut decoder = FrameDecoder::new();
    let mut documents = Vec::new();
    for byte in stream {
        documents.extend(decoder.push(&[byte]).expect("valid frames"));
    }
    let decoded = documents.iter().map(|d| text(d)).collect::<Vec<_>>();
    assert_eq!(decoded, ["x", "yz"]);
}

#[test]
fn a_non_decimal_length_fails_the_stream() {
    let mut decoder = FrameDecoder::new();
    let failure = decoder
        .push(b"12a\npayload")
        .expect_err("non-decimal length");
    assert_eq!(failure.detail(), "frame length is not a decimal integer");
    // The decoder is poisoned: the stream has no resynchronization point.
    assert_eq!(decoder.push(&[1]).expect_err("poisoned"), failure);
}

#[test]
fn a_negative_or_padded_length_fails_the_stream() {
    assert!(FrameDecoder::new().push(b"-1\n").is_err());
    assert_eq!(
        FrameDecoder::new()
            .push(b"01\nx\n")
            .expect_err("leading zero")
            .detail(),
        "frame length has a leading zero"
    );
    assert_eq!(
        FrameDecoder::new()
            .push(b"\nx")
            .expect_err("empty length")
            .detail(),
        "frame length is empty"
    );
}

#[test]
fn an_unterminated_or_oversized_length_header_fails_the_stream() {
    assert_eq!(
        FrameDecoder::new()
            .push(b"1234567890123456")
            .expect_err("unterminated header")
            .detail(),
        "frame length header is not terminated"
    );
    assert_eq!(
        FrameDecoder::new()
            .push(b"1234567890123456\nx")
            .expect_err("oversized header")
            .detail(),
        "frame length header is too long"
    );
}

#[test]
fn the_longest_accepted_length_header_is_fifteen_digits() {
    let mut decoder = FrameDecoder::new();
    // 15 digits is accepted as a header; the payload simply never arrives.
    assert!(decoder
        .push(b"999999999999999\n")
        .expect("header")
        .is_empty());
    assert!(decoder.has_partial_frame());
}

#[test]
fn a_payload_without_its_terminator_fails_the_stream() {
    assert_eq!(
        FrameDecoder::new()
            .push(b"2\nabX")
            .expect_err("missing terminator")
            .detail(),
        "frame payload is not terminated"
    );
}

#[test]
fn finish_rejects_a_stream_that_ends_inside_a_frame() {
    let mut decoder = FrameDecoder::new();
    decoder.push(b"5\nab").expect("partial frame");
    assert_eq!(
        decoder.finish().expect_err("truncated stream").detail(),
        "stream ended inside a frame"
    );

    let mut clean = FrameDecoder::new();
    clean.push(&encode_frame(b"done")).expect("valid frame");
    clean.finish().expect("clean boundary");
}
