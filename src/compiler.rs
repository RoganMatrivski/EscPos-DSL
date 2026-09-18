use base64::Engine;
use escpos::printer::Printer;
use escpos::utils::*;
use std::fmt;
use winnow::ascii::space0;
use winnow::combinator::{alt, delimited, repeat};
use winnow::token::{literal, take_till, take_until, take_while};
use winnow::{ModalResult, Parser};

use crate::driver::MemoryDriver;

pub const DEFAULT_MAX_CHARS_PER_LINE: u8 = 32; // Default 32 chars for 58mm printer
use crate::image_helper::DitherAlgo;

#[derive(Debug, PartialEq, Eq)]
pub enum CompileError {
    PrinterError(String),
    ImageDecodeError(String),
    ImageDitherError(String),
    ImageSizeMismatch {
        expected_w: u32,
        expected_h: u32,
        actual_w: u32,
        actual_h: u32,
    },
}

impl fmt::Display for CompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CompileError::PrinterError(s) => write!(f, "Printer error: {}", s),
            CompileError::ImageDecodeError(s) => write!(f, "Image decode error: {}", s),
            CompileError::ImageDitherError(s) => write!(f, "Image dither error: {}", s),
            CompileError::ImageSizeMismatch {
                expected_w,
                expected_h,
                actual_w,
                actual_h,
            } => write!(
                f,
                "Image size mismatch: expected {}x{}, got {}x{}",
                expected_w, expected_h, actual_w, actual_h
            ),
        }
    }
}

impl std::error::Error for CompileError {}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Token<'a> {
    BoldOpen,
    BoldClose,
    UnderlineOpen,
    UnderlineClose,
    RightOpen,
    RightClose,
    AutoSpace {
        max: Option<u8>,
    },
    LPadOpen {
        len: usize,
        fill: char,
    },
    LPadSelfClosing {
        len: usize,
        fill: char,
    },
    LPadClose,
    RPadOpen {
        len: usize,
        fill: char,
    },
    RPadSelfClosing {
        len: usize,
        fill: char,
    },
    RPadClose,
    Hr {
        len: Option<usize>,
        fill: char,
    },
    SizeOpen {
        w: u8,
        h: u8,
    },
    SizeClose,
    Pos(u16),
    QrCode {
        size: u8,
        data: &'a str,
    },
    Pdf417(&'a str),
    Img {
        w: Option<u32>,
        h: Option<u32>,
        dither: Option<DitherAlgo>,
        b64: &'a str,
    },
    UnknownTag,
    Text(&'a str),
}

pub fn compile(dsl: &str, max_chars_per_line: u8) -> Result<Vec<u8>, CompileError> {
    let max_chars = if max_chars_per_line == 0 {
        DEFAULT_MAX_CHARS_PER_LINE
    } else {
        max_chars_per_line
    };

    let driver = MemoryDriver::new();
    let mut printer = Printer::new(driver.clone(), Protocol::default(), None);

    // Initial printer reset
    printer
        .init()
        .map_err(|e| CompileError::PrinterError(e.to_string()))?;

    let lines: Vec<&str> = dsl.split('\n').collect();
    let mut base_alignment = JustifyMode::LEFT;

    for (line_idx, raw_line) in lines.iter().enumerate() {
        let line = raw_line.trim_end_matches('\r');

        // Check for comment line (# preceded by optional whitespace)
        let mut input = line;
        if is_comment_line(&mut input) {
            continue;
        }

        // Parse line-level alignment prefix ([L], [C], [R])
        if let Some(align) = parse_line_prefix(&mut input) {
            base_alignment = align;
        }

        // Apply base alignment for current line
        let mut current_alignment = base_alignment;
        printer
            .justify(current_alignment)
            .map_err(|e| CompileError::PrinterError(e.to_string()))?;

        let mut alignment_stack: Vec<JustifyMode> = Vec::new();
        let mut size_stack: Vec<(u8, u8)> = Vec::new();
        let mut current_size = (1u8, 1u8);
        let mut in_bold = false;
        let mut in_underline = false;

        parse_and_emit_line(
            input,
            max_chars,
            &mut printer,
            &mut base_alignment,
            &mut current_alignment,
            &mut alignment_stack,
            &mut current_size,
            &mut size_stack,
            &mut in_bold,
            &mut in_underline,
        )?;

        // Line end resets bold, underline, and text size
        if in_bold {
            printer
                .bold(false)
                .map_err(|e| CompileError::PrinterError(e.to_string()))?;
        }
        if in_underline {
            printer
                .underline(UnderlineMode::None)
                .map_err(|e| CompileError::PrinterError(e.to_string()))?;
        }
        if current_size != (1, 1) {
            printer
                .size(1, 1)
                .map_err(|e| CompileError::PrinterError(e.to_string()))?;
        }

        // Emit LF for every line (including blank lines), unless it's the trailing empty line from split
        if line_idx < lines.len() - 1 || !line.is_empty() {
            printer
                .feed()
                .map_err(|e| CompileError::PrinterError(e.to_string()))?;
        }
    }

    // Flush all commands to the driver
    printer
        .print_cut()
        .map_err(|e| CompileError::PrinterError(e.to_string()))?;

    Ok(driver.get_bytes())
}

fn is_comment_line(input: &mut &str) -> bool {
    let mut check_input = *input;
    let res: ModalResult<()> = (space0, literal('#'))
        .map(|_| ())
        .parse_next(&mut check_input);
    res.is_ok()
}

fn parse_line_prefix(input: &mut &str) -> Option<JustifyMode> {
    let mut align_parser = alt((
        literal("[L]").value(JustifyMode::LEFT),
        literal("[C]").value(JustifyMode::CENTER),
        literal("[R]").value(JustifyMode::RIGHT),
    ));
    let res: ModalResult<JustifyMode> = align_parser.parse_next(input);
    res.ok()
}

fn parse_token<'i>(input: &mut &'i str) -> ModalResult<Token<'i>> {
    alt((
        alt((
            literal("<b>").value(Token::BoldOpen),
            literal("</b>").value(Token::BoldClose),
            literal("<u>").value(Token::UnderlineOpen),
            literal("</u>").value(Token::UnderlineClose),
            alt((
                literal("<r>"),
                literal("<r-align>"),
                literal("<r/>"),
                literal("<r-align/>"),
            ))
            .value(Token::RightOpen),
            alt((literal("</r>"), literal("</r-align>"))).value(Token::RightClose),
            parse_autospace_tag,
            parse_lpad_tag,
            alt((
                literal("</l-pad>"),
                literal("</lpad>"),
                literal("</left-pad>"),
            ))
            .value(Token::LPadClose),
            parse_rpad_tag,
            alt((
                literal("</r-pad>"),
                literal("</rpad>"),
                literal("</right-pad>"),
            ))
            .value(Token::RPadClose),
            parse_hr_tag,
        )),
        alt((
            alt((
                literal("<big>"),
                literal("<double-size>"),
                literal("<d>"),
                literal("<h1>"),
                literal("<h2>"),
            ))
            .value(Token::SizeOpen { w: 2, h: 2 }),
            alt((
                literal("</big>"),
                literal("</double-size>"),
                literal("</d>"),
                literal("<h1>"),
                literal("<h2>"),
            ))
            .value(Token::SizeClose),
            alt((literal("<dh>"), literal("<double-height>")))
                .value(Token::SizeOpen { w: 1, h: 2 }),
            alt((literal("</dh>"), literal("</double-height>"))).value(Token::SizeClose),
            alt((literal("<dw>"), literal("<double-width>"))).value(Token::SizeOpen { w: 2, h: 1 }),
            alt((literal("</dw>"), literal("</double-width>"))).value(Token::SizeClose),
            parse_size_tag,
            literal("</size>").value(Token::SizeClose),
            parse_pos_tag,
            parse_qrcode_tag,
            parse_pdf417_tag,
            parse_img_tag,
            parse_unknown_tag,
            parse_text_token,
        )),
    ))
    .parse_next(input)
}

fn parse_autospace_tag<'i>(input: &mut &'i str) -> ModalResult<Token<'i>> {
    let _ = literal("<autospace").parse_next(input)?;
    let body = take_until(0.., ">").parse_next(input)?;
    let _ = literal(">").parse_next(input)?;

    let max = parse_attribute(body, "max").and_then(|s| s.parse::<u8>().ok());

    Ok(Token::AutoSpace { max })
}

fn parse_lpad_tag<'i>(input: &mut &'i str) -> ModalResult<Token<'i>> {
    let _ = alt((literal("<l-pad"), literal("<lpad"), literal("<left-pad"))).parse_next(input)?;
    let body = take_until(0.., ">").parse_next(input)?;
    let _ = literal(">").parse_next(input)?;

    let len = parse_attribute(body, "len")
        .or_else(|| parse_attribute(body, "length"))
        .or_else(|| parse_attribute(body, "w"))
        .or_else(|| parse_attribute(body, "width"))
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(0);

    let fill = parse_attribute(body, "ch")
        .or_else(|| parse_attribute(body, "char"))
        .or_else(|| parse_attribute(body, "fill"))
        .and_then(|s| s.chars().next())
        .unwrap_or(' ');

    if body.trim_end().ends_with('/') {
        Ok(Token::LPadSelfClosing { len, fill })
    } else {
        Ok(Token::LPadOpen { len, fill })
    }
}

fn parse_rpad_tag<'i>(input: &mut &'i str) -> ModalResult<Token<'i>> {
    let _ = alt((literal("<r-pad"), literal("<rpad"), literal("<right-pad"))).parse_next(input)?;
    let body = take_until(0.., ">").parse_next(input)?;
    let _ = literal(">").parse_next(input)?;

    let len = parse_attribute(body, "len")
        .or_else(|| parse_attribute(body, "length"))
        .or_else(|| parse_attribute(body, "w"))
        .or_else(|| parse_attribute(body, "width"))
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(0);

    let fill = parse_attribute(body, "ch")
        .or_else(|| parse_attribute(body, "char"))
        .or_else(|| parse_attribute(body, "fill"))
        .and_then(|s| s.chars().next())
        .unwrap_or(' ');

    if body.trim_end().ends_with('/') {
        Ok(Token::RPadSelfClosing { len, fill })
    } else {
        Ok(Token::RPadOpen { len, fill })
    }
}

fn parse_hr_tag<'i>(input: &mut &'i str) -> ModalResult<Token<'i>> {
    let _ = alt((literal("<hr"), literal("<horizontal-rule"))).parse_next(input)?;
    let body = take_until(0.., ">").parse_next(input)?;
    let _ = literal(">").parse_next(input)?;

    let len = parse_attribute(body, "len")
        .or_else(|| parse_attribute(body, "length"))
        .or_else(|| parse_attribute(body, "w"))
        .or_else(|| parse_attribute(body, "width"))
        .or_else(|| parse_attribute(body, "count"))
        .and_then(|s| s.parse::<usize>().ok());

    let fill = parse_attribute(body, "ch")
        .or_else(|| parse_attribute(body, "char"))
        .or_else(|| parse_attribute(body, "fill"))
        .and_then(|s| s.chars().next())
        .unwrap_or('-');

    Ok(Token::Hr { len, fill })
}

fn parse_size_tag<'i>(input: &mut &'i str) -> ModalResult<Token<'i>> {
    let _ = literal("<size").parse_next(input)?;
    let body = take_until(0.., ">").parse_next(input)?;
    let _ = literal(">").parse_next(input)?;

    let w = parse_attribute(body, "w")
        .and_then(|s| s.parse::<u8>().ok())
        .unwrap_or(1)
        .clamp(1, 8);

    let h = parse_attribute(body, "h")
        .and_then(|s| s.parse::<u8>().ok())
        .unwrap_or(1)
        .clamp(1, 8);

    Ok(Token::SizeOpen { w, h })
}

fn parse_pos_tag<'i>(input: &mut &'i str) -> ModalResult<Token<'i>> {
    let _ = literal("<pos").parse_next(input)?;
    let body = take_until(0.., ">").parse_next(input)?;
    let _ = literal(">").parse_next(input)?;

    let x = parse_attribute(body, "x")
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(0);

    Ok(Token::Pos(x))
}

fn parse_qrcode_tag<'i>(input: &mut &'i str) -> ModalResult<Token<'i>> {
    let _ = literal("<qrcode").parse_next(input)?;
    let header = take_until(0.., ">").parse_next(input)?;
    let _ = literal(">").parse_next(input)?;

    let data = take_until(0.., "</qrcode>").parse_next(input)?;
    let _ = literal("</qrcode>").parse_next(input)?;

    let size = parse_attribute(header, "size")
        .and_then(|s| s.parse::<u8>().ok())
        .unwrap_or(6);

    Ok(Token::QrCode { size, data })
}

fn parse_pdf417_tag<'i>(input: &mut &'i str) -> ModalResult<Token<'i>> {
    let _ = literal("<pdf417").parse_next(input)?;
    let _ = take_until(0.., ">").parse_next(input)?;
    let _ = literal(">").parse_next(input)?;

    let data = take_until(0.., "</pdf417>").parse_next(input)?;
    let _ = literal("</pdf417>").parse_next(input)?;

    Ok(Token::Pdf417(data))
}

fn parse_img_tag<'i>(input: &mut &'i str) -> ModalResult<Token<'i>> {
    let _ = literal("<img").parse_next(input)?;
    let header = take_until(0.., ">").parse_next(input)?;
    let _ = literal(">").parse_next(input)?;

    let b64 = take_until(0.., "</img>").parse_next(input)?;
    let _ = literal("</img>").parse_next(input)?;

    let w = parse_attribute(header, "w").and_then(|s| s.parse::<u32>().ok());
    let h = parse_attribute(header, "h").and_then(|s| s.parse::<u32>().ok());
    let dither = parse_attribute(header, "dither").and_then(DitherAlgo::from_str);

    Ok(Token::Img { w, h, dither, b64 })
}

fn parse_unknown_tag<'i>(input: &mut &'i str) -> ModalResult<Token<'i>> {
    let _ = literal("<").parse_next(input)?;
    let _ = take_until(0.., ">").parse_next(input)?;
    let _ = literal(">").parse_next(input)?;
    Ok(Token::UnknownTag)
}

fn parse_text_token<'i>(input: &mut &'i str) -> ModalResult<Token<'i>> {
    let text = take_till(1.., '<').parse_next(input)?;
    Ok(Token::Text(text))
}

fn parse_line_tokens<'i>(mut input: &'i str) -> Vec<Token<'i>> {
    let mut parser = repeat(0.., parse_token);
    let res: ModalResult<Vec<Token<'i>>> = parser.parse_next(&mut input);
    res.unwrap_or_default()
}

fn count_token_chars(tokens: &[Token], current_width_scale: u8) -> usize {
    let mut total_chars = 0usize;
    let mut width_scale = current_width_scale;
    let mut size_stack = Vec::new();

    let mut i = 0;
    while i < tokens.len() {
        match &tokens[i] {
            Token::Text(t) => {
                total_chars += t.chars().count() * (width_scale as usize);
            }
            Token::SizeOpen { w, .. } => {
                size_stack.push(width_scale);
                width_scale = *w;
            }
            Token::SizeClose => {
                width_scale = size_stack.pop().unwrap_or(current_width_scale);
            }
            Token::LPadSelfClosing { len, .. } | Token::RPadSelfClosing { len, .. } => {
                total_chars += len * (width_scale as usize);
            }
            Token::Hr { len, .. } => {
                if let Some(l) = len {
                    total_chars += l * (width_scale as usize);
                }
            }
            Token::LPadOpen { len, .. } => {
                let mut j = i + 1;
                while j < tokens.len() && tokens[j] != Token::LPadClose {
                    j += 1;
                }
                let inner_tokens = &tokens[i + 1..j];
                let inner_chars = count_token_chars(inner_tokens, width_scale);
                let target_len = len * (width_scale as usize);
                total_chars += std::cmp::max(target_len, inner_chars);
                i = j;
            }
            Token::RPadOpen { len, .. } => {
                let mut j = i + 1;
                while j < tokens.len() && tokens[j] != Token::RPadClose {
                    j += 1;
                }
                let inner_tokens = &tokens[i + 1..j];
                let inner_chars = count_token_chars(inner_tokens, width_scale);
                let target_len = len * (width_scale as usize);
                total_chars += std::cmp::max(target_len, inner_chars);
                i = j;
            }
            Token::RightOpen => {
                let mut j = i + 1;
                while j < tokens.len() && tokens[j] != Token::RightClose {
                    j += 1;
                }
                let inner_tokens = &tokens[i + 1..j];
                let inner_chars = count_token_chars(inner_tokens, width_scale);
                total_chars += inner_chars;
                i = j;
            }
            Token::BoldOpen
            | Token::BoldClose
            | Token::UnderlineOpen
            | Token::UnderlineClose
            | Token::LPadClose
            | Token::RPadClose
            | Token::RightClose
            | Token::AutoSpace { .. }
            | Token::Pos(_)
            | Token::QrCode { .. }
            | Token::Pdf417(_)
            | Token::Img { .. }
            | Token::UnknownTag => {}
        }
        i += 1;
    }
    total_chars
}

fn process_inner_token(
    inner_token: &Token,
    printer: &mut Printer<MemoryDriver>,
    in_bold: &mut bool,
    in_underline: &mut bool,
    current_size: &mut (u8, u8),
    size_stack: &mut Vec<(u8, u8)>,
    printed_text_on_line: &mut bool,
) -> Result<(), CompileError> {
    match inner_token {
        Token::Text(t) => {
            if !t.is_empty() {
                printer
                    .write(t)
                    .map_err(|e| CompileError::PrinterError(e.to_string()))?;
                *printed_text_on_line = true;
            }
        }
        Token::BoldOpen => {
            *in_bold = true;
            printer
                .bold(true)
                .map_err(|e| CompileError::PrinterError(e.to_string()))?;
        }
        Token::BoldClose => {
            *in_bold = false;
            printer
                .bold(false)
                .map_err(|e| CompileError::PrinterError(e.to_string()))?;
        }
        Token::UnderlineOpen => {
            *in_underline = true;
            printer
                .underline(UnderlineMode::Single)
                .map_err(|e| CompileError::PrinterError(e.to_string()))?;
        }
        Token::UnderlineClose => {
            *in_underline = false;
            printer
                .underline(UnderlineMode::None)
                .map_err(|e| CompileError::PrinterError(e.to_string()))?;
        }
        Token::SizeOpen { w, h } => {
            size_stack.push(*current_size);
            *current_size = (*w, *h);
            printer
                .size(*w, *h)
                .map_err(|e| CompileError::PrinterError(e.to_string()))?;
        }
        Token::SizeClose => {
            let (rw, rh) = size_stack.pop().unwrap_or((1, 1));
            *current_size = (rw, rh);
            printer
                .size(rw, rh)
                .map_err(|e| CompileError::PrinterError(e.to_string()))?;
        }
        Token::Pos(x) => {
            let n_l = (x & 0xFF) as u8;
            let n_h = ((x >> 8) & 0xFF) as u8;
            printer
                .custom(&[0x1b, 0x24, n_l, n_h])
                .map_err(|e| CompileError::PrinterError(e.to_string()))?;
        }
        _ => {}
    }
    Ok(())
}

fn parse_and_emit_line(
    input: &str,
    max_chars_per_line: u8,
    printer: &mut Printer<MemoryDriver>,
    base_alignment: &mut JustifyMode,
    current_alignment: &mut JustifyMode,
    alignment_stack: &mut Vec<JustifyMode>,
    current_size: &mut (u8, u8),
    size_stack: &mut Vec<(u8, u8)>,
    in_bold: &mut bool,
    in_underline: &mut bool,
) -> Result<(), CompileError> {
    let tokens = parse_line_tokens(input);
    let mut printed_text_on_line = false;

    let mut i = 0;
    while i < tokens.len() {
        let token = &tokens[i];
        match token {
            Token::BoldOpen => {
                *in_bold = true;
                printer
                    .bold(true)
                    .map_err(|e| CompileError::PrinterError(e.to_string()))?;
            }
            Token::BoldClose => {
                *in_bold = false;
                printer
                    .bold(false)
                    .map_err(|e| CompileError::PrinterError(e.to_string()))?;
            }
            Token::UnderlineOpen => {
                *in_underline = true;
                printer
                    .underline(UnderlineMode::Single)
                    .map_err(|e| CompileError::PrinterError(e.to_string()))?;
            }
            Token::UnderlineClose => {
                *in_underline = false;
                printer
                    .underline(UnderlineMode::None)
                    .map_err(|e| CompileError::PrinterError(e.to_string()))?;
            }
            Token::AutoSpace { max } => {
                let target_max = max.unwrap_or(max_chars_per_line) as usize;
                let left_chars = count_token_chars(&tokens[..i], current_size.0);
                let right_chars = count_token_chars(&tokens[i + 1..], current_size.0);
                let spaces_needed = target_max.saturating_sub(left_chars + right_chars);

                if spaces_needed > 0 {
                    let spaces = " ".repeat(spaces_needed);
                    printer
                        .write(&spaces)
                        .map_err(|e| CompileError::PrinterError(e.to_string()))?;
                }
            }
            Token::Hr { len, fill } => {
                let count = len.unwrap_or(max_chars_per_line as usize);
                if count > 0 {
                    let hr_str = fill.to_string().repeat(count);
                    printer
                        .write(&hr_str)
                        .map_err(|e| CompileError::PrinterError(e.to_string()))?;
                    printed_text_on_line = true;
                }
            }
            Token::LPadSelfClosing { len, fill } => {
                if *len > 0 {
                    let padding = fill.to_string().repeat(*len);
                    printer
                        .write(&padding)
                        .map_err(|e| CompileError::PrinterError(e.to_string()))?;
                    printed_text_on_line = true;
                }
            }
            Token::LPadOpen { len, fill } => {
                let mut j = i + 1;
                while j < tokens.len() && tokens[j] != Token::LPadClose {
                    j += 1;
                }
                let inner_tokens = &tokens[i + 1..j];
                let inner_chars = count_token_chars(inner_tokens, current_size.0);
                let padding_len = len.saturating_sub(inner_chars);

                if padding_len > 0 {
                    let padding = fill.to_string().repeat(padding_len);
                    printer
                        .write(&padding)
                        .map_err(|e| CompileError::PrinterError(e.to_string()))?;
                }

                for inner_token in inner_tokens {
                    process_inner_token(
                        inner_token,
                        printer,
                        in_bold,
                        in_underline,
                        current_size,
                        size_stack,
                        &mut printed_text_on_line,
                    )?;
                }

                i = j;
            }
            Token::LPadClose => {}
            Token::RPadSelfClosing { len, fill } => {
                if *len > 0 {
                    let padding = fill.to_string().repeat(*len);
                    printer
                        .write(&padding)
                        .map_err(|e| CompileError::PrinterError(e.to_string()))?;
                    printed_text_on_line = true;
                }
            }
            Token::RPadOpen { len, fill } => {
                let mut j = i + 1;
                while j < tokens.len() && tokens[j] != Token::RPadClose {
                    j += 1;
                }
                let inner_tokens = &tokens[i + 1..j];
                let inner_chars = count_token_chars(inner_tokens, current_size.0);
                let padding_len = len.saturating_sub(inner_chars);

                for inner_token in inner_tokens {
                    process_inner_token(
                        inner_token,
                        printer,
                        in_bold,
                        in_underline,
                        current_size,
                        size_stack,
                        &mut printed_text_on_line,
                    )?;
                }

                if padding_len > 0 {
                    let padding = fill.to_string().repeat(padding_len);
                    printer
                        .write(&padding)
                        .map_err(|e| CompileError::PrinterError(e.to_string()))?;
                }

                i = j;
            }
            Token::RPadClose => {}
            Token::RightOpen => {
                if !printed_text_on_line {
                    // Line-level right align
                    alignment_stack.push(*current_alignment);
                    *current_alignment = JustifyMode::RIGHT;
                    printer
                        .justify(JustifyMode::RIGHT)
                        .map_err(|e| CompileError::PrinterError(e.to_string()))?;
                } else {
                    // Mid-line right align: pad with whitespace to max_chars_per_line
                    let mut right_tokens = Vec::new();
                    let mut j = i + 1;
                    while j < tokens.len() && tokens[j] != Token::RightClose {
                        right_tokens.push(tokens[j].clone());
                        j += 1;
                    }

                    let left_chars = count_token_chars(&tokens[..i], current_size.0);
                    let right_chars = count_token_chars(&right_tokens, current_size.0);
                    let spaces_needed =
                        (max_chars_per_line as usize).saturating_sub(left_chars + right_chars);

                    if spaces_needed > 0 {
                        let spaces = " ".repeat(spaces_needed);
                        printer
                            .write(&spaces)
                            .map_err(|e| CompileError::PrinterError(e.to_string()))?;
                    }

                    // Process inner tokens inside <r>
                    for inner_token in &right_tokens {
                        process_inner_token(
                            inner_token,
                            printer,
                            in_bold,
                            in_underline,
                            current_size,
                            size_stack,
                            &mut printed_text_on_line,
                        )?;
                    }

                    // Skip tokens through RightClose
                    i = j;
                }
            }
            Token::RightClose => {
                let restore_align = alignment_stack.pop().unwrap_or(*base_alignment);
                *current_alignment = restore_align;
                printer
                    .justify(restore_align)
                    .map_err(|e| CompileError::PrinterError(e.to_string()))?;
            }
            Token::SizeOpen { w, h } => {
                size_stack.push(*current_size);
                *current_size = (*w, *h);
                printer
                    .size(*w, *h)
                    .map_err(|e| CompileError::PrinterError(e.to_string()))?;
            }
            Token::SizeClose => {
                let (restore_w, restore_h) = size_stack.pop().unwrap_or((1, 1));
                *current_size = (restore_w, restore_h);
                printer
                    .size(restore_w, restore_h)
                    .map_err(|e| CompileError::PrinterError(e.to_string()))?;
            }
            Token::Pos(x) => {
                let n_l = (x & 0xFF) as u8;
                let n_h = ((x >> 8) & 0xFF) as u8;
                printer
                    .custom(&[0x1b, 0x24, n_l, n_h])
                    .map_err(|e| CompileError::PrinterError(e.to_string()))?;
            }
            Token::QrCode { size, data } => {
                let opt = QRCodeOption::new(QRCodeModel::Model2, *size, QRCodeCorrectionLevel::L);
                printer
                    .qrcode_option(data, opt)
                    .map_err(|e| CompileError::PrinterError(e.to_string()))?;
                printed_text_on_line = true;
            }
            Token::Pdf417(data) => {
                printer
                    .pdf417(data)
                    .map_err(|e| CompileError::PrinterError(e.to_string()))?;
                printed_text_on_line = true;
            }
            Token::Img { w, h, dither, b64 } => {
                let b64_data = b64.trim();
                let img_bytes = base64::engine::general_purpose::STANDARD
                    .decode(b64_data)
                    .map_err(|e| {
                        let preview_len = std::cmp::min(b64_data.len(), 50);
                        let preview = &b64_data[..preview_len];
                        CompileError::ImageDecodeError(format!(
                            "{} (base64 len: {}, prefix: {:?})",
                            e,
                            b64_data.len(),
                            preview
                        ))
                    })?;

                let mut final_bytes = img_bytes;
                if let Ok(mut dyn_img) = image::load_from_memory(&final_bytes) {
                    let mut modified = false;

                    if let (Some(ew), Some(eh)) = (w, h) {
                        let (aw, ah) = (dyn_img.width(), dyn_img.height());
                        if aw != *ew || ah != *eh {
                            dyn_img = dyn_img.resize_exact(
                                *ew,
                                *eh,
                                image::imageops::FilterType::Lanczos3,
                            );
                            modified = true;
                        }
                    }

                    if let Some(algo) = dither {
                        let mut gray = crate::image_helper::to_luma_white_bg(&dyn_img);
                        algo.apply(&mut gray)
                            .map_err(CompileError::ImageDitherError)?;
                        dyn_img = image::DynamicImage::ImageLuma8(gray);
                        modified = true;
                    }

                    if modified {
                        let mut png_bytes = Vec::new();
                        if dyn_img
                            .write_to(
                                &mut std::io::Cursor::new(&mut png_bytes),
                                image::ImageFormat::Png,
                            )
                            .is_ok()
                        {
                            final_bytes = png_bytes;
                        }
                    }
                }

                printer
                    .bit_image_from_bytes(&final_bytes)
                    .map_err(|e| CompileError::PrinterError(e.to_string()))?;
                printed_text_on_line = true;
            }
            Token::UnknownTag => {
                // Ignore unknown tags
            }
            Token::Text(text) => {
                if !text.is_empty() {
                    printer
                        .write(text)
                        .map_err(|e| CompileError::PrinterError(e.to_string()))?;
                    printed_text_on_line = true;
                }
            }
        }
        i += 1;
    }

    Ok(())
}

fn parse_quoted_val<'i>(input: &mut &'i str) -> ModalResult<&'i str> {
    delimited('"', take_until(0.., "\""), '"').parse_next(input)
}

fn parse_sq_quoted_val<'i>(input: &mut &'i str) -> ModalResult<&'i str> {
    delimited('\'', take_until(0.., "'"), '\'').parse_next(input)
}

fn parse_unquoted_val<'i>(input: &mut &'i str) -> ModalResult<&'i str> {
    take_while(0.., |c: char| !c.is_whitespace() && c != '/').parse_next(input)
}

fn parse_single_attr<'i>(attr_name: &str, input: &mut &'i str) -> ModalResult<&'i str> {
    let _ = take_until(0.., attr_name).parse_next(input)?;
    let _ = (literal(attr_name), space0, literal('=')).parse_next(input)?;
    alt((parse_quoted_val, parse_sq_quoted_val, parse_unquoted_val)).parse_next(input)
}

pub fn parse_attribute<'a>(tag_body: &'a str, attr_name: &str) -> Option<&'a str> {
    let mut input = tag_body;
    parse_single_attr(attr_name, &mut input).ok()
}
