use crate::tree;
use crosstermion::ansi_term::{ANSIString, ANSIStrings, Color, Style};
use std::{io, ops::RangeInclusive};
use unicode_width::UnicodeWidthStr;

#[derive(Default)]
pub struct State {
    tree: Vec<(tree::Key, tree::Value)>,
    messages: Vec<tree::Message>,
    from_copying: Option<tree::MessageCopyState>,
    max_message_origin_size: usize,
    /// The amount of blocks per line we have written last time.
    blocks_per_line: Vec<u16>,
    /// Amount of times we drew so far
    ticks: usize,
}

pub struct Options {
    pub level_filter: Option<RangeInclusive<tree::Level>>,
    pub keep_running_if_progress_is_empty: bool,
    pub output_is_terminal: bool,
    pub colored: bool,
    pub timestamp: bool,
}

fn messages(out: &mut impl io::Write, state: &mut State, colored: bool, timestamp: bool) -> io::Result<()> {
    let mut brush = crosstermion::color::Brush::new(colored);
    fn to_color(level: tree::MessageLevel) -> Color {
        use tree::MessageLevel::*;
        match level {
            Info => Color::White,
            Success => Color::Green,
            Failure => Color::Red,
        }
    }
    for (
        tree::Message {
            time,
            level,
            origin,
            message,
        },
        last_drawn_line_length,
    ) in state
        .messages
        .iter()
        .zip(state.blocks_per_line.iter().chain(std::iter::repeat(&0)))
    {
        let message_block_len = origin.width();
        state.max_message_origin_size = state.max_message_origin_size.max(message_block_len);
        let color = to_color(*level);
        writeln!(
            out,
            " {}{} {}{:>overdraw_size$}",
            if timestamp {
                format!(
                    "{} ",
                    brush
                        .style(color.dimmed().on(Color::Yellow))
                        .paint(crate::time::format_time_for_messages(*time)),
                )
            } else {
                "".into()
            },
            brush.style(Style::default().dimmed()).paint(format!(
                "{:>fill_size$}{}",
                "",
                origin,
                fill_size = state.max_message_origin_size - message_block_len,
            )),
            brush.style(color.bold()).paint(message),
            "",
            overdraw_size = 0,
        )?;
    }
    Ok(())
}

pub fn all(out: &mut impl io::Write, progress: &tree::Root, state: &mut State, config: &Options) -> io::Result<()> {
    progress.sorted_snapshot(&mut state.tree);
    if !config.keep_running_if_progress_is_empty && state.tree.is_empty() {
        return Err(io::Error::new(io::ErrorKind::Other, "stop as progress is empty"));
    }
    state.from_copying = Some(progress.copy_new_messages(&mut state.messages, state.from_copying.take()));
    messages(out, state, config.colored, config.timestamp)?;

    if config.output_is_terminal {
        let level_range = config
            .level_filter
            .clone()
            .unwrap_or(RangeInclusive::new(0, tree::Level::max_value()));
        let lines_to_be_drawn = state
            .tree
            .iter()
            .filter(|(k, _)| level_range.contains(&k.level()))
            .count();
        if state.blocks_per_line.len() < lines_to_be_drawn {
            state.blocks_per_line.resize(lines_to_be_drawn, 0);
        }
        let mut tokens: Vec<ANSIString<'_>> = Vec::new();
        for ((key, progress), ref mut blocks_in_last_iteration) in state
            .tree
            .iter()
            .filter(|(k, _)| level_range.contains(&k.level()))
            .zip(state.blocks_per_line.iter_mut())
        {
            tokens.clear();
            format_progress(key, progress, state.ticks, &mut tokens);
            write!(out, "{}", ANSIStrings(tokens.as_slice()))?;

            let current_block_count = block_count_sans_ansi_codes(&tokens);
            if **blocks_in_last_iteration > current_block_count {
                // fill to the end of line to overwrite what was previously there
                writeln!(
                    out,
                    "{:>width$}",
                    "",
                    width = (**blocks_in_last_iteration - current_block_count) as usize
                )?;
            } else {
                writeln!(out)?;
            }
            **blocks_in_last_iteration = current_block_count;
        }
        // overwrite remaining lines that we didn't touch naturally
        if state.blocks_per_line.len() > lines_to_be_drawn {
            for blocks_in_last_iteration in &state.blocks_per_line[lines_to_be_drawn..] {
                writeln!(out, "{:>width$}", "", width = *blocks_in_last_iteration as usize)?;
            }
            // Move cursor back to end of the portion we have actually drawn
            crosstermion::execute!(out, crosstermion::cursor::MoveUp(state.blocks_per_line.len() as u16))?;
            state.blocks_per_line.resize(lines_to_be_drawn, 0);
        } else {
            crosstermion::execute!(out, crosstermion::cursor::MoveUp(lines_to_be_drawn as u16))?;
        }
    }
    state.ticks += 1;
    Ok(())
}

fn block_count_sans_ansi_codes(strings: &[ANSIString<'_>]) -> u16 {
    strings.iter().map(|s| s.width() as u16).sum()
}

fn format_progress<'a>(key: &tree::Key, progress: &'a tree::Value, ticks: usize, buf: &mut Vec<ANSIString<'a>>) {
    buf.push(Style::new().paint(format!("{:>level$}", "", level = key.level() as usize)));
    buf.push(Color::Yellow.paint(format!("{}", ticks)));
    buf.push(Color::Green.on(Color::Red).paint(&progress.name));
    buf.push(Style::new().paint("long text long text long text long text long text long text long text long text long text long text long text "));
}
