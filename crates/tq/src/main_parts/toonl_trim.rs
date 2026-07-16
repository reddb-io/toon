fn trim_toonl_keep_last(input: &str, keep_last: usize) -> Result<TrimPlan, String> {
    validate_toonl(input)?;
    let scan = scan_toonl_trim_units(input)?;

    if scan.rows.len() <= keep_last {
        return Ok(TrimPlan {
            output: input.to_owned(),
            changed: false,
        });
    }

    let (headers, suffix_start) = if keep_last == 0 {
        (scan.live_headers_at_end.clone(), input.len())
    } else {
        let cut_index = scan.rows.len() - keep_last;
        let cut = &scan.rows[cut_index];
        (cut.live_headers.clone(), cut.start)
    };

    let mut output = String::new();
    for header in &headers {
        output.push_str(&line_with_lf(header));
    }
    if keep_last == 0 {
        if scan
            .last_anonymous_segment
            .and_then(|segment| scan.segments.get(segment))
            .and_then(|segment| segment.trailer)
            .is_some()
        {
            output.push_str("[=0]\n");
        }
    } else {
        append_trimmed_suffix(input, suffix_start, &scan, &mut output);
    }
    validate_toonl(&output)?;

    Ok(TrimPlan {
        changed: output != input,
        output,
    })
}

#[derive(Debug)]
struct TrimScan {
    segments: Vec<TrimSegment>,
    rows: Vec<TrimRow>,
    live_headers_at_end: Vec<String>,
    last_anonymous_segment: Option<usize>,
}

fn validate_toonl(input: &str) -> Result<(), String> {
    for row in ToonlReader::new(Cursor::new(input.as_bytes())) {
        row.map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn scan_toonl_trim_units(input: &str) -> Result<TrimScan, String> {
    let mut segments: Vec<TrimSegment> = Vec::new();
    let mut rows: Vec<TrimRow> = Vec::new();
    let mut current: Option<usize> = None;
    let mut last_segment: Option<usize> = None;
    let mut live_headers = LiveHeaders::default();
    let mut offset = 0;
    let mut line_number = 0;

    while offset < input.len() {
        let line_start = offset;
        let line_end = input[offset..]
            .find('\n')
            .map(|index| offset + index + 1)
            .unwrap_or(input.len());
        offset = line_end;
        line_number += 1;

        let raw_line = &input[line_start..line_end];
        let line = raw_line.trim_end_matches('\n').trim_end_matches('\r');
        if line.is_empty() {
            continue;
        }
        if is_toonl_trailer(line) {
            let segment = current
                .take()
                .ok_or_else(|| format!("line {line_number}: trailer without header"))?;
            segments[segment].trailer = Some((line_start, line_end));
            continue;
        }
        if let Some(header) = parse_toonl_trim_header(line) {
            match header {
                TrimHeader::Continuation => {}
                TrimHeader::Anonymous => {
                    let segment = segments.len();
                    segments.push(TrimSegment {
                        header_start: line_start,
                        trailer: None,
                    });
                    current = Some(segment);
                    last_segment = Some(segment);
                    live_headers.set_anonymous(raw_line.to_owned());
                }
                TrimHeader::Tagged(tag) => {
                    live_headers.set_tagged(tag, raw_line.to_owned());
                }
            }
            continue;
        }

        let anonymous_segment = if is_toonl_tagged_row(line, &live_headers) {
            None
        } else {
            Some(current.ok_or_else(|| format!("line {line_number}: row before header"))?)
        };
        rows.push(TrimRow {
            start: line_start,
            live_headers: live_headers.lines(),
            anonymous_segment,
        });
    }

    Ok(TrimScan {
        segments,
        rows,
        live_headers_at_end: live_headers.lines(),
        last_anonymous_segment: last_segment,
    })
}

fn is_toonl_trailer(line: &str) -> bool {
    line.starts_with("[=") && line.ends_with(']')
}

fn parse_toonl_trim_header(line: &str) -> Option<TrimHeader> {
    let rest = line.strip_prefix('[')?;
    let close_bracket = rest.find(']')?;
    let bracket = &rest[..close_bracket];
    let continuation = bracket.starts_with('~');
    let delimiter = if continuation { &bracket[1..] } else { bracket };
    if !matches!(delimiter, "" | "|" | "\t") {
        return None;
    }
    let mut suffix = &rest[close_bracket + 1..];
    if continuation {
        return if suffix.starts_with('{') && suffix.ends_with("}:") {
            Some(TrimHeader::Continuation)
        } else {
            None
        };
    }
    if let Some(after_open) = suffix.strip_prefix('<') {
        let tag_end = after_open.find('>')?;
        let tag = &after_open[..tag_end];
        suffix = &after_open[tag_end + 1..];
        return if suffix.starts_with('{') && suffix.ends_with("}:") {
            Some(TrimHeader::Tagged(tag.to_owned()))
        } else {
            None
        };
    }
    if suffix.starts_with('{') && suffix.ends_with("}:") {
        Some(TrimHeader::Anonymous)
    } else {
        None
    }
}

fn is_toonl_tagged_row(line: &str, live_headers: &LiveHeaders) -> bool {
    let Some(colon) = line.find(':') else {
        return false;
    };
    if colon == 0 {
        return false;
    }
    let tag = &line[..colon];
    live_headers.has_tag(tag)
        && tag
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

fn append_trimmed_suffix(input: &str, suffix_start: usize, scan: &TrimScan, output: &mut String) {
    let mut cursor = suffix_start;
    for (segment_index, segment) in scan.segments.iter().enumerate() {
        let Some((trailer_start, trailer_end)) = segment.trailer else {
            continue;
        };
        if trailer_start < suffix_start || segment.header_start >= suffix_start {
            continue;
        }
        output.push_str(&input[cursor..trailer_start]);
        let retained = scan
            .rows
            .iter()
            .filter(|row| row.start >= suffix_start && row.anonymous_segment == Some(segment_index))
            .count();
        output.push_str(&format!("[={retained}]\n"));
        cursor = trailer_end;
    }
    output.push_str(&input[cursor..]);
}

#[derive(Debug)]
enum TrimHeader {
    Anonymous,
    Continuation,
    Tagged(String),
}

#[derive(Debug, Default)]
struct LiveHeaders {
    order: Vec<LiveHeaderKey>,
    anonymous: Option<String>,
    tagged: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum LiveHeaderKey {
    Anonymous,
    Tagged(String),
}

impl LiveHeaders {
    fn set_anonymous(&mut self, header: String) {
        if self.anonymous.is_none() {
            self.order.push(LiveHeaderKey::Anonymous);
        }
        self.anonymous = Some(header);
    }

    fn set_tagged(&mut self, tag: String, header: String) {
        if let Some((_, existing)) = self
            .tagged
            .iter_mut()
            .find(|(existing_tag, _)| existing_tag == &tag)
        {
            *existing = header;
            return;
        }
        self.order.push(LiveHeaderKey::Tagged(tag.clone()));
        self.tagged.push((tag, header));
    }

    fn has_tag(&self, tag: &str) -> bool {
        self.tagged
            .iter()
            .any(|(existing_tag, _)| existing_tag == tag)
    }

    fn lines(&self) -> Vec<String> {
        self.order
            .iter()
            .filter_map(|key| match key {
                LiveHeaderKey::Anonymous => self.anonymous.clone(),
                LiveHeaderKey::Tagged(tag) => self
                    .tagged
                    .iter()
                    .find(|(existing_tag, _)| existing_tag == tag)
                    .map(|(_, header)| header.clone()),
            })
            .collect()
    }
}

fn line_with_lf(line: &str) -> String {
    if line.ends_with('\n') {
        line.to_owned()
    } else {
        format!("{line}\n")
    }
}

fn write_in_place_atomically(path: &str, bytes: &[u8]) -> Result<(), String> {
    let path = Path::new(path);
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "input path must name a file".to_owned())?;

    let mut last_error = None;
    for attempt in 0..100 {
        let tmp_path = parent.join(format!(
            ".{file_name}.tq-trim.{}.{}.tmp",
            process::id(),
            attempt
        ));
        match write_temp_then_rename(path, &tmp_path, bytes) {
            Ok(()) => return Ok(()),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                last_error = Some(error);
            }
            Err(error) => {
                let _ = fs::remove_file(&tmp_path);
                return Err(format!("{}: {error}", path.display()));
            }
        }
    }

    Err(format!(
        "{}: could not create temporary trim file: {}",
        path.display(),
        last_error
            .map(|error| error.to_string())
            .unwrap_or_else(|| "too many collisions".to_owned())
    ))
}

fn write_temp_then_rename(path: &Path, tmp_path: &PathBuf, bytes: &[u8]) -> io::Result<()> {
    {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(tmp_path)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    fs::rename(tmp_path, path)
}

