use super::{Format, FormatEvent, FormatFields, FormatTime};
use crate::{
    field::{RecordFields, VisitOutput},
    fmt::fmt_layer::{FmtContext, FormattedFields},
    registry::LookupSpan,
};
use serde::ser::{SerializeMap, Serializer as _};
use serde_json::Serializer;
use std::{
    collections::BTreeMap,
    fmt::{self, Write},
    io,
};
use tracing_core::{
    field::{self, Field},
    span::Record,
    Event, Subscriber,
};
use tracing_serde::AsSerde;

#[cfg(feature = "tracing-log")]
use tracing_log::NormalizeEvent;

/// Marker for `Format` that indicates that the verbose json log format should be used.
///
/// The full format includes fields from all entered spans.
///
/// # Example Output
///
/// ```ignore,json
/// {"timestamp":"Feb 20 11:28:15.096","level":"INFO","target":"mycrate","fields":{"message":"some message", "key": "value"}}
/// ```
///
/// # Options
///
/// - [`Json::flatten_event`] can be used to enable flattening event fields into the root
/// object.
///
/// [`Json::flatten_event`]: #method.flatten_event
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct Json {
    pub(crate) flatten_event: bool,
}

impl Json {
    /// If set to `true` event metadata will be flattened into the root object.
    pub fn flatten_event(&mut self, flatten_event: bool) {
        self.flatten_event = flatten_event;
    }
}

impl<S, N, T> FormatEvent<S, N> for Format<Json, T>
where
    S: Subscriber + for<'lookup> LookupSpan<'lookup>,
    N: for<'writer> FormatFields<'writer> + 'static,
    T: FormatTime,
{
    fn format_event(
        &self,
        ctx: &FmtContext<'_, S, N>,
        writer: &mut dyn fmt::Write,
        event: &Event<'_>,
    ) -> fmt::Result
    where
        S: Subscriber + for<'a> LookupSpan<'a>,
    {
        use serde_json::json;
        let mut timestamp = String::new();
        self.timer.format_time(&mut timestamp)?;

        #[cfg(feature = "tracing-log")]
        let normalized_meta = event.normalized_metadata();
        #[cfg(feature = "tracing-log")]
        let meta = normalized_meta.as_ref().unwrap_or_else(|| event.metadata());
        #[cfg(not(feature = "tracing-log"))]
        let meta = event.metadata();

        let flatten_event = self.format.flatten_event;

        let mut visit = || {
            let mut serializer = Serializer::new(WriteAdaptor::new(writer));

            let mut serializer = serializer.serialize_map(None)?;

            serializer.serialize_entry("timestamp", &timestamp)?;
            serializer.serialize_entry("level", &meta.level().as_serde())?;

            let id = ctx.ctx.current_span();
            let id = id.id();
            if let Some(id) = id {
                if let Some(span) = ctx.ctx.span(id) {
                    let ext = span.extensions();
                    let data = ext
                        .get::<FormattedFields<N>>()
                        .expect("Unable to find FormattedFields in extensions; this is a bug");
                    // TODO: let's _not_ do this, but this resolves
                    // https://github.com/tokio-rs/tracing/issues/391.
                    // We should probably rework this to use a `serde_json::Value` or something
                    // similar in a JSON-specific layer, but I'd (david)
                    // rather have a uglier fix now rather than shipping broken JSON.
                    let mut fields = match serde_json::from_str(&data) {
                        Ok(fields) => fields,
                        // We have previously recorded fields for this span
                        // should be valid JSON. However, they appear to *not*
                        // be valid JSON. This is almost certainly a bug, so
                        // panic if we're in debug mode
                        Err(e) if cfg!(debug_assertions) => panic!(
                            "span '{}' had malformed fields! this is a bug.\n  error: {}\n  fields: {:?}",
                            span.metadata().name(),
                            e,
                            data
                        ),
                        // If we *aren't* in debug mode, it's probably best not
                        // crash the program, but let's at least make sure it's clear
                        // that the fields are not supposed to be missing.
                        Err(e) => json!({ "field_error": format!("{}", e) }),
                    };
                    fields["name"] = json!(span.metadata().name());
                    serializer.serialize_entry("span", &fields).unwrap_or(());
                }
            }

            if self.display_target {
                serializer.serialize_entry("target", meta.target())?;
            }

            if !flatten_event {
                use tracing_serde::fields::AsMap;
                serializer.serialize_entry("fields", &event.field_map())?;
                serializer.end()
            } else {
                let mut visitor = tracing_serde::SerdeMapVisitor::new(serializer);
                event.record(&mut visitor);

                visitor.finish()
            }
        };

        visit().map_err(|_| fmt::Error)?;
        writeln!(writer)
    }
}

impl Default for Json {
    fn default() -> Json {
        Json {
            flatten_event: false,
        }
    }
}

/// The JSON [`FormatFields`] implementation.
///
/// [`FormatFields`]: trait.FormatFields.html
#[derive(Debug)]
pub struct JsonFields {
    // reserve the ability to add fields to this without causing a breaking
    // change in the future.
    _private: (),
}

impl JsonFields {
    /// Returns a new JSON [`FormatFields`] implementation.
    ///
    /// [`FormatFields`]: trait.FormatFields.html
    pub fn new() -> Self {
        Self { _private: () }
    }
}

impl Default for JsonFields {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> FormatFields<'a> for JsonFields {
    /// Format the provided `fields` to the provided `writer`, returning a result.
    fn format_fields<R: RecordFields>(
        &self,
        writer: &'a mut dyn fmt::Write,
        fields: R,
    ) -> fmt::Result {
        let mut v = JsonVisitor::new(writer);
        fields.record(&mut v);
        v.finish()
    }

    /// Record additional field(s) on an existing span.
    ///
    /// By default, this appends a space to the current set of fields if it is
    /// non-empty, and then calls `self.format_fields`. If different behavior is
    /// required, the default implementation of this method can be overridden.
    fn add_fields(&self, current: &'a mut String, fields: &Record<'_>) -> fmt::Result {
        if !current.is_empty() {
            // If fields were previously recorded on this span, we need to parse
            // the current set of fields as JSON, add the new fields, and
            // re-serialize them. Otherwise, if we just appended the new fields
            // to a previously serialized JSON object, we would end up with
            // malformed JSON.
            //
            // XXX(eliza): this is far from efficient, but unfortunately, it is
            // necessary as long as the JSON formatter is implemented on top of
            // an interface that stores all formatted fields as strings.
            //
            // We should consider reimplementing the JSON formatter as a
            // separate layer, rather than a formatter for the `fmt` layer ???
            // then, we could store fields as JSON values, and add to them
            // without having to parse and re-serialize.
            let mut new = String::new();
            let map: BTreeMap<&'_ str, serde_json::Value> =
                serde_json::from_str(current).map_err(|_| fmt::Error)?;
            let mut v = JsonVisitor::new(&mut new);
            v.values = map;
            fields.record(&mut v);
            v.finish()?;
            *current = new;
        } else {
            // If there are no previously recorded fields, we can just reuse the
            // existing string.
            let mut v = JsonVisitor::new(current);
            fields.record(&mut v);
            v.finish()?;
        }

        Ok(())
    }
}

/// The [visitor] produced by [`JsonFields`]'s [`MakeVisitor`] implementation.
///
/// [visitor]: ../../field/trait.Visit.html
/// [`JsonFields`]: struct.JsonFields.html
/// [`MakeVisitor`]: ../../field/trait.MakeVisitor.html
pub struct JsonVisitor<'a> {
    values: BTreeMap<&'a str, serde_json::Value>,
    writer: &'a mut dyn Write,
}

impl<'a> fmt::Debug for JsonVisitor<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_fmt(format_args!("JsonVisitor {{ values: {:?} }}", self.values))
    }
}

impl<'a> JsonVisitor<'a> {
    /// Returns a new default visitor that formats to the provided `writer`.
    ///
    /// # Arguments
    /// - `writer`: the writer to format to.
    /// - `is_empty`: whether or not any fields have been previously written to
    ///   that writer.
    pub fn new(writer: &'a mut dyn Write) -> Self {
        Self {
            values: BTreeMap::new(),
            writer,
        }
    }
}

impl<'a> crate::field::VisitFmt for JsonVisitor<'a> {
    fn writer(&mut self) -> &mut dyn fmt::Write {
        self.writer
    }
}

impl<'a> crate::field::VisitOutput<fmt::Result> for JsonVisitor<'a> {
    fn finish(self) -> fmt::Result {
        let inner = || {
            let mut serializer = Serializer::new(WriteAdaptor::new(self.writer));
            let mut ser_map = serializer.serialize_map(None)?;

            for (k, v) in self.values {
                ser_map.serialize_entry(k, &v)?;
            }

            ser_map.end()
        };

        if inner().is_err() {
            Err(fmt::Error)
        } else {
            Ok(())
        }
    }
}

impl<'a> field::Visit for JsonVisitor<'a> {
    /// Visit a signed 64-bit integer value.
    fn record_i64(&mut self, field: &Field, value: i64) {
        self.values
            .insert(&field.name(), serde_json::Value::from(value));
    }

    /// Visit an unsigned 64-bit integer value.
    fn record_u64(&mut self, field: &Field, value: u64) {
        self.values
            .insert(&field.name(), serde_json::Value::from(value));
    }

    /// Visit a boolean value.
    fn record_bool(&mut self, field: &Field, value: bool) {
        self.values
            .insert(&field.name(), serde_json::Value::from(value));
    }

    /// Visit a string value.
    fn record_str(&mut self, field: &Field, value: &str) {
        self.values
            .insert(&field.name(), serde_json::Value::from(value));
    }

    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        match field.name() {
            // Skip fields that are actually log metadata that have already been handled
            #[cfg(feature = "tracing-log")]
            name if name.starts_with("log.") => (),
            name if name.starts_with("r#") => {
                self.values
                    .insert(&name[2..], serde_json::Value::from(format!("{:?}", value)));
            }
            name => {
                self.values
                    .insert(name, serde_json::Value::from(format!("{:?}", value)));
            }
        };
    }
}

/// A bridge between `fmt::Write` and `io::Write`.
///
/// This is needed because tracing-subscriber's FormatEvent expects a fmt::Write
/// while serde_json's Serializer expects an io::Write.
struct WriteAdaptor<'a> {
    fmt_write: &'a mut dyn fmt::Write,
}

impl<'a> WriteAdaptor<'a> {
    fn new(fmt_write: &'a mut dyn fmt::Write) -> Self {
        Self { fmt_write }
    }
}

impl<'a> io::Write for WriteAdaptor<'a> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let s =
            std::str::from_utf8(buf).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        self.fmt_write
            .write_str(&s)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

        Ok(s.as_bytes().len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<'a> fmt::Debug for WriteAdaptor<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad("WriteAdaptor { .. }")
    }
}

#[cfg(test)]
mod test {
    use crate::fmt::{test::MockWriter, time::FormatTime};
    use lazy_static::lazy_static;
    use tracing::{self, subscriber::with_default};

    use std::{fmt, sync::Mutex};

    struct MockTime;
    impl FormatTime for MockTime {
        fn format_time(&self, w: &mut dyn fmt::Write) -> fmt::Result {
            write!(w, "fake time")
        }
    }

    #[test]
    fn json() {
        lazy_static! {
            static ref BUF: Mutex<Vec<u8>> = Mutex::new(vec![]);
        }

        let make_writer = || MockWriter::new(&BUF);

        let expected =
        "{\"timestamp\":\"fake time\",\"level\":\"INFO\",\"span\":{\"answer\":42,\"name\":\"json_span\",\"number\":3},\"target\":\"tracing_subscriber::fmt::format::json::test\",\"fields\":{\"message\":\"some json test\"}}\n";

        test_json(make_writer, expected, &BUF, false);
    }

    #[test]
    fn json_flattened_event() {
        lazy_static! {
            static ref BUF: Mutex<Vec<u8>> = Mutex::new(vec![]);
        }

        let make_writer = || MockWriter::new(&BUF);

        let expected =
        "{\"timestamp\":\"fake time\",\"level\":\"INFO\",\"span\":{\"answer\":42,\"name\":\"json_span\",\"number\":3},\"target\":\"tracing_subscriber::fmt::format::json::test\",\"message\":\"some json test\"}\n";

        test_json(make_writer, expected, &BUF, true);
    }

    #[test]
    fn record_works() {
        // This test reproduces issue #707, where using `Span::record` causes
        // any events inside the span to be ignored.
        lazy_static! {
            static ref BUF: Mutex<Vec<u8>> = Mutex::new(vec![]);
        }

        let make_writer = || MockWriter::new(&BUF);
        let subscriber = crate::fmt().json().with_writer(make_writer).finish();

        let parse_buf = || -> serde_json::Value {
            let buf = String::from_utf8(BUF.try_lock().unwrap().to_vec()).unwrap();
            let json = buf
                .lines()
                .last()
                .expect("expected at least one line to be written!");
            match serde_json::from_str(&json) {
                Ok(v) => v,
                Err(e) => panic!(
                    "assertion failed: JSON shouldn't be malformed\n  error: {}\n  json: {}",
                    e, json
                ),
            }
        };

        with_default(subscriber, || {
            tracing::info!("an event outside the root span");
            assert_eq!(
                parse_buf()["fields"]["message"],
                "an event outside the root span"
            );

            let span = tracing::info_span!("the span", na = tracing::field::Empty);
            span.record("na", &"value");
            let _enter = span.enter();

            tracing::info!("an event inside the root span");
            assert_eq!(
                parse_buf()["fields"]["message"],
                "an event inside the root span"
            );
        });
    }

    #[cfg(feature = "json")]
    fn test_json<T>(make_writer: T, expected: &str, buf: &Mutex<Vec<u8>>, flatten_event: bool)
    where
        T: crate::fmt::MakeWriter + Send + Sync + 'static,
    {
        let subscriber = crate::fmt::Subscriber::builder()
            .json()
            .flatten_event(flatten_event)
            .with_writer(make_writer)
            .with_timer(MockTime)
            .finish();

        with_default(subscriber, || {
            let span = tracing::span!(tracing::Level::INFO, "json_span", answer = 42, number = 3);
            let _guard = span.enter();
            tracing::info!("some json test");
        });

        let actual = String::from_utf8(buf.try_lock().unwrap().to_vec()).unwrap();
        assert_eq!(expected, actual.as_str());
    }
}
