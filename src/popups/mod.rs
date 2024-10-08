use anyhow::Result;
use crossterm::event::Event;
use ratatui::text::Line;
use ratatui::{
	layout::{Alignment, Rect},
	text::Span,
	widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap},
	Frame,
};
use std::{
	cell::{Cell}};

///
#[derive(PartialEq, Eq)]
pub enum EventState {
	Consumed,
	NotConsumed,
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
    pub fn new() -> Self {
		Self {
			title: String::new(),
			msg: String::new(),
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
					theme.text(true, false),
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