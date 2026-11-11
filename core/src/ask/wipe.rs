//! Overwriting memory that held a screenshot before it is given back.
//!
//! A screenshot is in memory only, and for as short as possible
//! ([`crate::vision`]). "Dropped" is not "gone": freed memory keeps
//! its contents until something else is written there. So every buffer
//! of this crate that held the image, or the image as base64 text, is
//! overwritten with zeros first.
//!
//! What this can and cannot do:
//!
//! - It wipes the buffers this crate owns, in full, at the moment they
//!   are let go.
//! - It cannot wipe a copy it does not own (inside the HTTP client,
//!   in a pipe, in another process), and it cannot wipe what an
//!   allocator left behind when a buffer was moved to grow. The
//!   buffers wiped here are made at their final size, so there is no
//!   such leftover from this crate's own code.

use std::sync::atomic::{Ordering, compiler_fence};

use serde_json::Value;

/// Overwrite `buf` with zeros, in a way the optimiser may not drop
/// although nothing reads the buffer again.
pub(crate) fn bytes(buf: &mut [u8]) {
    buf.fill(0);
    // The buffer is "read" here as far as the compiler can tell, so
    // the store above is not dead, also right before a free.
    std::hint::black_box(&*buf);
    compiler_fence(Ordering::SeqCst);
}

/// Overwrite the text of `text` with zeros and leave it empty.
pub(crate) fn string(text: &mut String) {
    let mut raw = std::mem::take(text).into_bytes();
    bytes(&mut raw);
}

/// Overwrite every string value in `value` (not the keys) and leave
/// them empty: for a JSON message that carried an image as text.
pub(crate) fn json(value: &mut Value) {
    match value {
        Value::String(text) => string(text),
        Value::Array(items) => items.iter_mut().for_each(json),
        Value::Object(map) => map.values_mut().for_each(json),
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}

/// A text that is wiped when it is dropped: the line or the request
/// body that carries an image on its way out.
pub(crate) struct Text(pub String);

impl Drop for Text {
    fn drop(&mut self) {
        string(&mut self.0);
    }
}

/// A JSON value that is wiped when it is dropped.
pub(crate) struct Json(pub Value);

impl Drop for Json {
    fn drop(&mut self) {
        json(&mut self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn bytes_and_strings_are_overwritten_in_place() {
        let mut buf = vec![7u8; 4096];
        bytes(&mut buf);
        assert!(buf.iter().all(|b| *b == 0));
        assert_eq!(buf.len(), 4096);
        bytes(&mut []);

        // The very allocation is overwritten, not a copy of it: the
        // bytes are looked at through the pointer taken before.
        let mut text = String::from("aVZCT1J3MEtHZ28=");
        let (at, len) = (text.as_ptr(), text.len());
        // SAFETY: `text` owns `len` bytes at `at` until it is dropped,
        // and it is not dropped inside this block.
        let seen = |text: &String| {
            assert_eq!(text.as_ptr(), at);
            unsafe { std::slice::from_raw_parts(at, len) }.to_vec()
        };
        assert_eq!(seen(&text), b"aVZCT1J3MEtHZ28=");
        // `string` takes the buffer out and wipes it; here the wipe is
        // done in place so the test can look at the result.
        // SAFETY: zeros are valid UTF-8.
        bytes(unsafe { text.as_mut_vec() });
        assert_eq!(seen(&text), vec![0u8; len]);

        let mut text = String::from("data:image/png;base64,iVBORw==");
        string(&mut text);
        assert!(text.is_empty());
    }

    #[test]
    fn every_string_in_a_json_value_is_wiped_and_the_shape_stays() {
        let mut message = json!({
            "role": "user",
            "content": [
                { "type": "text", "text": "what does this say" },
                { "type": "image_url", "image_url": { "url": "data:image/png;base64,iVBORw==" } },
            ],
            "n": 3,
            "more": null,
        });
        json(&mut message);
        assert_eq!(
            message,
            json!({
                "role": "",
                "content": [
                    { "type": "", "text": "" },
                    { "type": "", "image_url": { "url": "" } },
                ],
                "n": 3,
                "more": null,
            })
        );
    }

    #[test]
    fn the_guards_wipe_what_they_hold_when_they_go() {
        // Dropping must not panic or leave the value behind; what a
        // guard does is the functions above, called from `drop`.
        let text = Text("iVBORw==".repeat(64));
        assert_eq!(text.0.len(), 512);
        drop(text);
        let body = Json(json!({ "url": "data:image/png;base64,iVBORw==" }));
        assert!(body.0["url"].as_str().unwrap().starts_with("data:"));
        drop(body);
        // The guards really have a destructor of their own.
        #[allow(drop_bounds)]
        fn wipes_on_drop<T: Drop>() {}
        wipes_on_drop::<Text>();
        wipes_on_drop::<Json>();
        wipes_on_drop::<crate::ask::AskImage>();
    }
}
