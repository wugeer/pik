use std::{borrow::Cow, rc::Rc};

use crossterm::event::KeyEvent;
use ratatui::{
    layout::{Alignment, Constraint, Layout, Margin, Rect},
    style::{palette::tailwind, Color, Modifier, Style, Stylize},
    text::{Line, Span, Text},
    widgets::{
        block::{Position, Title},
        Block, BorderType, Borders, HighlightSpacing, Paragraph, Row, Scrollbar,
        ScrollbarOrientation, ScrollbarState, Table, TableState, Wrap,
    },
    Frame,
};
use tui_textarea::TextArea;
use anyhow::Result;
use std::cell::Cell;
use crate::processes::{Process, ProcessSearchResults, SearchBy};

pub struct Theme {
    row_fg: Color,
    selected_style_fg: Color,
    normal_row_color: Color,
    alt_row_color: Color,
    process_table_border_color: Color,
}

impl Theme {
    pub fn new() -> Self {
        Self {
            row_fg: tailwind::SLATE.c200,
            selected_style_fg: tailwind::BLUE.c400,
            normal_row_color: tailwind::SLATE.c950,
            alt_row_color: tailwind::SLATE.c900,
            process_table_border_color: tailwind::BLUE.c400,
        }
    }
}

pub struct Tui {
    theme: Theme,
    process_table: TableState,
    process_table_scroll_state: ScrollbarState,
    process_table_number_of_items: usize,
    process_details_scroll_state: ScrollbarState,
    process_details_scroll_offset: u16,
    process_details_number_of_lines: u16,
    search_area: TextArea<'static>,
    error_message: Option<&'static str>,
}

impl Tui {
    pub fn new(search_text: String) -> Self {
        let mut search_area = TextArea::from(search_text.lines());
        search_area.move_cursor(tui_textarea::CursorMove::End);
        Self {
            process_table: TableState::default(),
            process_table_scroll_state: ScrollbarState::new(0),
            theme: Theme::new(),
            process_table_number_of_items: 0,
            process_details_scroll_offset: 0,
            process_details_number_of_lines: 0,
            //NOTE: we don't update this, value 1 means that this should be rendered
            process_details_scroll_state: ScrollbarState::new(1),
            search_area,
            error_message: None,
        }
    }

    pub fn select_next_row(&mut self, step_size: usize) {
        let next_row_index = self.process_table.selected().map(|i| {
            let mut i = i + step_size;
            if i >= self.process_table_number_of_items {
                i = 0
            }
            i
        });
        self.process_table.select(next_row_index);
        self.process_table_scroll_state = self
            .process_table_scroll_state
            .position(next_row_index.unwrap_or(0));
        self.reset_process_detals_scroll();
    }

    pub fn select_previous_row(&mut self, step_size: usize) {
        let previous_index = self.process_table.selected().map(|i| {
            let i = i.wrapping_sub(step_size);
            i.clamp(0, self.process_table_number_of_items.saturating_sub(1))
        });
        self.process_table.select(previous_index);
        self.process_table_scroll_state = self
            .process_table_scroll_state
            .position(previous_index.unwrap_or(0));
        self.reset_process_detals_scroll();
    }

    pub fn handle_input(&mut self, input: KeyEvent) {
        self.search_area.input(input);
    }

    pub fn enter_char(&mut self, new_char: char) {
        self.search_area.insert_char(new_char);
    }

    pub fn process_details_down(&mut self, frame: &mut Frame) {
        let rects = layout_rects(frame);
        let process_details_area = rects[2];
        let area_content_height = process_details_area.height - 2;
        let content_scrolled =
            self.process_details_number_of_lines - self.process_details_scroll_offset;

        if content_scrolled > area_content_height {
            self.process_details_scroll_offset =
                self.process_details_scroll_offset.saturating_add(1);
        }
    }

    pub fn process_details_up(&mut self) {
        self.process_details_scroll_offset = self.process_details_scroll_offset.saturating_sub(1);
    }

    fn reset_process_detals_scroll(&mut self) {
        self.process_details_scroll_offset = 0;
    }

    pub fn set_error_message(&mut self, message: &'static str) {
        self.error_message = Some(message);
    }

    pub fn reset_error_message(&mut self) {
        self.error_message = None;
    }

    pub fn delete_char(&mut self) {
        self.search_area.delete_char();
    }

    pub fn get_selected_row_index(&self) -> Option<usize> {
        self.process_table.selected()
    }

    pub fn update_process_table_number_of_items(&mut self, number_of_items: usize) {
        self.process_table_number_of_items = number_of_items;
        self.process_table_scroll_state = self
            .process_table_scroll_state
            .content_length(number_of_items.saturating_sub(1));
        if number_of_items == 0 {
            self.process_table.select(None);
        } else {
            self.process_table.select(Some(0));
        }
    }

    pub fn search_input_text(&self) -> &str {
        &self.search_area.lines()[0]
    }

    pub fn render_ui(&mut self, search_results: &ProcessSearchResults, frame: &mut Frame) {
        let rects = layout_rects(frame);

        self.render_search_input(frame, rects[0]);
        self.render_process_table(frame, search_results, rects[1]);
        self.render_process_details(frame, search_results, rects[2]);

        render_help(frame, self.error_message, rects[3]);
    }

    fn render_search_input(&self, f: &mut Frame, area: Rect) {
        let rects = Layout::horizontal([Constraint::Length(2), Constraint::Min(2)]).split(area);
        f.render_widget(Paragraph::new("> "), rects[0]);
        f.render_widget(&self.search_area, rects[1]);
    }

    fn render_process_table(
        &mut self,
        f: &mut Frame,
        search_results: &ProcessSearchResults,
        area: Rect,
    ) {
        let (dynamic_header, value_getter) = dynamic_search_column(search_results);
        let rows = search_results.iter().enumerate().map(|(i, data)| {
            let color = match i % 2 {
                0 => self.theme.normal_row_color,
                _ => self.theme.alt_row_color,
            };
            Row::new(vec![
                Cow::Borrowed(data.user_name.as_str()),
                Cow::Owned(format!("{}", data.pid)),
                Cow::Owned(data.parent_as_string()),
                Cow::Borrowed(&data.start_time),
                Cow::Borrowed(&data.run_time),
                Cow::Borrowed(&data.cmd),
                Cow::Borrowed(data.cmd_path.as_deref().unwrap_or("")),
                Cow::Borrowed(value_getter(data)),
            ])
            .style(Style::new().fg(self.theme.row_fg).bg(color))
        });
        let table = Table::new(
            rows,
            [
                Constraint::Percentage(5),
                Constraint::Percentage(5),
                Constraint::Percentage(5),
                Constraint::Percentage(5),
                Constraint::Percentage(5),
                Constraint::Percentage(10),
                Constraint::Percentage(25),
                Constraint::Percentage(40),
            ],
        )
        .header(Row::new(vec![
            "USER",
            "PID",
            "PARENT",
            "STARTED",
            "TIME",
            "CMD",
            "CMD_PATH",
            dynamic_header,
        ]))
        .block(
            Block::default()
                .title(
                    Title::from(format!(
                        " {} / {} ",
                        self.process_table.selected().map(|i| i + 1).unwrap_or(0),
                        search_results.len()
                    ))
                    .position(Position::Top)
                    .alignment(Alignment::Left),
                )
                .borders(Borders::ALL)
                .border_style(Style::new().fg(self.theme.process_table_border_color))
                .border_type(BorderType::Plain),
        )
        .highlight_style(
            Style::default()
                .add_modifier(Modifier::REVERSED)
                .fg(self.theme.selected_style_fg),
        )
        .highlight_symbol(Text::from(vec![" ".into()]))
        .highlight_spacing(HighlightSpacing::Always);
        f.render_stateful_widget(table, area, &mut self.process_table);
        f.render_stateful_widget(
            Scrollbar::default()
                .orientation(ScrollbarOrientation::VerticalRight)
                .begin_symbol(None)
                .end_symbol(None),
            area.inner(Margin {
                vertical: 1,
                horizontal: 1,
            }),
            &mut self.process_table_scroll_state,
        );
    }

    fn render_process_details(
        &mut self,
        f: &mut Frame,
        search_results: &ProcessSearchResults,
        area: Rect,
    ) {
        let selected_process = search_results.nth(self.get_selected_row_index());
        let lines = process_details_lines(selected_process);

        self.update_process_details_number_of_lines(area, selected_process);

        let info_footer = Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .left_aligned()
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(
                        Title::from(" Process Details ")
                            .alignment(Alignment::Left)
                            .position(Position::Top),
                    )
                    // .border_style(Style::new().fg(app.colors.footer_border_color))
                    .border_type(BorderType::Rounded),
            )
            .scroll((self.process_details_scroll_offset, 0));
        f.render_widget(info_footer, area);
        f.render_stateful_widget(
            Scrollbar::default()
                .orientation(ScrollbarOrientation::VerticalRight)
                .thumb_symbol("")
                .track_symbol(None)
                .begin_symbol(Some("↑"))
                .end_symbol(Some("↓")),
            area,
            &mut self.process_details_scroll_state,
        );
    }

    fn update_process_details_number_of_lines(
        &mut self,
        area: Rect,
        selected_process: Option<&Process>,
    ) {
        let content_width = area.width - 2;

        match selected_process {
            Some(process) => {
                let args_number_of_lines =
                    (process.args.chars().count() as u16 / content_width) + 1;
                self.process_details_number_of_lines = args_number_of_lines + 2;
            }
            None => {
                self.process_details_number_of_lines = 1;
            }
        }
    }

    fn render_keybindings(&self, f: &mut Frame) {
        let rects = Layout::horizontal([Constraint::Length(2), Constraint::Min(2)]).split(area);
        f.render_widget(Paragraph::new("> "), rects[0]);
        f.render_widget(&self.search_area, rects[1]);
    }

}

fn dynamic_search_column(search_result: &ProcessSearchResults) -> (&str, fn(&Process) -> &str) {
    match search_result.search_by {
        SearchBy::Port => ("PORT", |prc| prc.ports.as_deref().unwrap_or("")),
        SearchBy::Args => ("ARGS", |prc| prc.args.as_str()),
        _ => ("", |_| ""),
    }
}

fn process_details_lines(selected_process: Option<&Process>) -> Vec<Line> {
    match selected_process {
        Some(prc) => {
            let ports = prc
                .ports
                .as_deref()
                .map(|p| format!(" PORTS: {}", p))
                .unwrap_or("".to_string());
            let parent = prc
                .parent_pid
                .map(|p| format!(" PARENT: {}", p))
                .unwrap_or("".to_string());
            vec![
                Line::from(format!(
                    "USER: {} PID: {}{} START_TIME: {}, RUN_TIME: {} MEMORY: {}MB{}",
                    prc.user_name,
                    prc.pid,
                    parent,
                    prc.start_time,
                    prc.run_time,
                    prc.memory / 1024 / 1024,
                    ports,
                )),
                Line::from(format!("CMD: {}", prc.exe())),
                //FIXME: Sometimes args are too long and don't fit in details area
                Line::from(format!("ARGS: {}", prc.args)),
            ]
        }
        None => vec![Line::from("No process selected")],
    }
}

const HELP_TEXT: &str =
    "ESC/<C+C> quit | <C+X> kill process | <C+R> refresh | <C+F> details forward | <C+B> details backward ";

fn render_help(f: &mut Frame, error_message: Option<&str>, area: Rect) {
    let rects = Layout::horizontal([Constraint::Percentage(25), Constraint::Percentage(75)])
        .horizontal_margin(1)
        .split(area);
    let error = Paragraph::new(Span::from(error_message.unwrap_or("")).fg(Color::Red))
        .left_aligned()
        .block(Block::default().borders(Borders::NONE));
    let help = Paragraph::new(Line::from(HELP_TEXT)).right_aligned();
    f.render_widget(error, rects[0]);
    f.render_widget(help, rects[1]);
}

fn layout_rects(frame: &mut Frame) -> Rc<[Rect]> {
    Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(10),
        Constraint::Max(7),
        Constraint::Length(1),
    ])
    .split(frame.area())
}

#[derive(Copy, Clone)]
pub enum ScrollType {
	Up,
	Down,
	Home,
	End,
	PageUp,
	PageDown,
}

pub struct VerticalScroll {
	top: Cell<usize>,
	max_top: Cell<usize>,
}

impl VerticalScroll {
	pub const fn new() -> Self {
		Self {
			top: Cell::new(0),
			max_top: Cell::new(0),
		}
	}

	pub fn get_top(&self) -> usize {
		self.top.get()
	}

	pub fn reset(&self) {
		self.top.set(0);
	}

	pub fn move_top(&self, move_type: ScrollType) -> bool {
		let old = self.top.get();
		let max = self.max_top.get();

		let new_scroll_top = match move_type {
			ScrollType::Down => old.saturating_add(1),
			ScrollType::Up => old.saturating_sub(1),
			ScrollType::Home => 0,
			ScrollType::End => max,
			_ => old,
		};

		let new_scroll_top = new_scroll_top.clamp(0, max);

		if new_scroll_top == old {
			return false;
		}

		self.top.set(new_scroll_top);

		true
	}

	pub fn move_area_to_visible(
		&self,
		height: usize,
		start: usize,
		end: usize,
	) {
		let top = self.top.get();
		let bottom = top + height;
		let max_top = self.max_top.get();
		// the top of some content is hidden
		if start < top {
			self.top.set(start);
			return;
		}
		// the bottom of some content is hidden and there is visible space available
		if end > bottom && start > top {
			let avail_space = start.saturating_sub(top);
			let diff = std::cmp::min(
				avail_space,
				end.saturating_sub(bottom),
			);
			let top = top.saturating_add(diff);
			self.top.set(std::cmp::min(max_top, top));
		}
	}

	pub fn update(
		&self,
		selection: usize,
		selection_max: usize,
		visual_height: usize,
	) -> usize {
		let new_top = calc_scroll_top(
			self.get_top(),
			visual_height,
			selection,
			selection_max,
		);
		self.top.set(new_top);

		if visual_height == 0 {
			self.max_top.set(0);
		} else {
			let new_max = selection_max.saturating_sub(visual_height);
			self.max_top.set(new_max);
		}

		new_top
	}

	pub fn update_no_selection(
		&self,
		line_count: usize,
		visual_height: usize,
	) -> usize {
		self.update(self.get_top(), line_count, visual_height)
	}
}


pub struct MsgPopup {
	title: String,
	msg: String,
    theme: Theme,
	visible: bool,
	scroll: VerticalScroll,
}

const POPUP_HEIGHT: u16 = 25;
const BORDER_WIDTH: u16 = 2;
const MINIMUM_WIDTH: u16 = 60;

impl MsgPopup {
    pub fn new(theme: Theme ) -> Self {
		Self {
			title: String::new(),
			msg: String::new(),
            theme,
			visible: false,
			scroll: VerticalScroll::new(),
		}
	}

	fn draw(&self, f: &mut Frame, _rect: Rect) -> Result<()> {
		if !self.visible {
			return Ok(());
		}

		let max_width = f.area().width.max(MINIMUM_WIDTH);

		// determine the maximum width of text block
		let width = self
			.msg
			.lines()
			.map(str::len)
			.max()
			.unwrap_or(0)
			.saturating_add(BORDER_WIDTH.into())
			.clamp(MINIMUM_WIDTH.into(), max_width.into())
			.try_into()
			.expect("can't fail because we're clamping to u16 value");

		let area = centered_rect_absolute(width, POPUP_HEIGHT, f.area());

		// Wrap lines and break words if there is not enough space
		let wrapped_msg = bwrap::wrap_maybrk!(
			&self.msg,
			area.width.saturating_sub(BORDER_WIDTH).into()
		);

		let msg_lines: Vec<String> =
			wrapped_msg.lines().map(String::from).collect();
		let line_num = msg_lines.len();

		let height = POPUP_HEIGHT
			.saturating_sub(BORDER_WIDTH)
			.min(f.area().height.saturating_sub(BORDER_WIDTH));

		let top =
			self.scroll.update_no_selection(line_num, height.into());

		let scrolled_lines = msg_lines
			.iter()
			.skip(top)
			.take(height.into())
			.map(|line| {
				Line::from(vec![Span::styled(
					line.clone(),
					self.theme.text(true, false),
				)])
			})
			.collect::<Vec<Line>>();

		f.render_widget(Clear, area);
		f.render_widget(
			Paragraph::new(scrolled_lines)
				.block(
					Block::default()
						.title(Span::styled(
							self.title.as_str(),
							self.theme.text_danger(),
						))
						.borders(Borders::ALL)
						.border_type(BorderType::Thick),
				)
				.alignment(Alignment::Left)
				.wrap(Wrap { trim: true }),
			area,
		);

		self.scroll.draw(f, area, &self.theme);

		Ok(())
	}

	pub fn event(&mut self, ev: &Event) {
		if self.visible {
			if let Event::Key(e) = ev {
				if key_match(e, self.key_config.keys.enter) {
					self.hide();
				} else if key_match(
					e,
					self.key_config.keys.popup_down,
				) {
					self.scroll.move_top(ScrollType::Down);
				} else if key_match(e, self.key_config.keys.popup_up)
				{
					self.scroll.move_top(ScrollType::Up);
				}
			}
		}
	}

	fn is_visible(&self) -> bool {
		self.visible
	}

	fn hide(&mut self) {
		self.visible = false;
	}

	fn show(&mut self) {
		self.visible = true;
	}

	fn set_new_msg(
		&mut self,
		msg: &str,
		title: String,
	) {
		self.title = title;
		self.msg = msg.to_string();
		self.scroll.reset();
		self.show()
	}

	///
	pub fn show_error(&mut self, msg: &str) {
		self.set_new_msg(
			msg,
			msg_title_error(),
		)
	}

	///
	pub fn show_key_bindings(&mut self, msg: &str) {
		self.set_new_msg(
			msg,
			msg_title_key_bindings(),
		)
	}
}

pub fn msg_title_error() -> String {
	"Error".to_string()
}

pub fn msg_title_key_bindings() -> String {
	"Keybindings".to_string()
}

pub fn centered_rect_absolute(
	width: u16,
	height: u16,
	r: Rect,
) -> Rect {
	Rect::new(
		(r.width.saturating_sub(width)) / 2,
		(r.height.saturating_sub(height)) / 2,
		width.min(r.width),
		height.min(r.height),
	)
}


const fn calc_scroll_top(
	current_top: usize,
	height_in_lines: usize,
	selection: usize,
	selection_max: usize,
) -> usize {
	if height_in_lines == 0 {
		return 0;
	}
	if selection_max <= height_in_lines {
		return 0;
	}

	if current_top + height_in_lines <= selection {
		selection.saturating_sub(height_in_lines) + 1
	} else if current_top > selection {
		selection
	} else {
		current_top
	}
}