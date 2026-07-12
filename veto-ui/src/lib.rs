use std::{
    cell::RefCell,
    collections::HashMap,
    io::{self, Write as _},
    rc::Rc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    buffer::Buffer,
    crossterm::{
        event::{self, Event, KeyCode, KeyEventKind},
        execute,
        terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
    },
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, Clear, Paragraph, Wrap},
};
use veto_graph::CommitNode;

pub mod layout;
use layout::Cell;

type Nodes = HashMap<String, Rc<RefCell<CommitNode>>>;

const LANE_W: usize = 10; // "● abc1234" is 9 chars wide
const ROW_H: usize = 2;
const LANE_COLORS: [Color; 6] = [
    Color::Cyan,
    Color::Magenta,
    Color::Green,
    Color::Yellow,
    Color::Blue,
    Color::LightRed,
];
const PULSE: [Color; 4] = [Color::White, Color::LightYellow, Color::Yellow, Color::LightCyan];

fn lane_color(lane: usize) -> Color {
    LANE_COLORS[lane % LANE_COLORS.len()]
}

/// Restores the terminal on drop (including on panic unwind).
struct TermGuard;
impl Drop for TermGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        let _ = io::stdout().flush();
    }
}

struct App<'a> {
    nodes: &'a Nodes,
    cells: Vec<Cell>,
    pos: HashMap<String, (usize, usize)>, // hash -> (lane, row)
    sel: usize,                           // index into cells
    scroll: (usize, usize),               // (x, y) in world chars
    popup: bool,
    tick: usize,
}

/// Take over the terminal and run the interactive commit-graph explorer.
/// j/k walk parent/child, h/l move across lanes, i shows metadata, q quits.
pub fn run(nodes: &Nodes) -> io::Result<()> {
    let cells = layout::layout(nodes);
    let pos = cells
        .iter()
        .map(|c| (c.hash.clone(), (c.lane, c.row)))
        .collect();
    let mut app = App {
        nodes,
        cells,
        pos,
        sel: 0, // cells sorted by row asc, so index 0 is a row-0 (newest) cell
        scroll: (0, 0),
        popup: false,
        tick: 0,
    };

    enable_raw_mode()?;
    execute!(io::stdout(), EnterAlternateScreen)?;
    let _guard = TermGuard;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;

    loop {
        terminal.draw(|f| app.draw(f))?;
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    KeyCode::Char('i') => app.popup = !app.popup,
                    _ if app.popup => app.popup = false, // any key closes the popup
                    KeyCode::Char('j') => app.move_kin(true),
                    KeyCode::Char('k') => app.move_kin(false),
                    KeyCode::Char('h') => app.move_lateral(-1),
                    KeyCode::Char('l') => app.move_lateral(1),
                    _ => {}
                }
            }
        } else {
            app.tick = app.tick.wrapping_add(1);
        }
    }
    Ok(())
}

impl App<'_> {
    fn sel_cell(&self) -> Option<&Cell> {
        self.cells.get(self.sel)
    }

    /// Hashes of parents (or children) of the selected node, via Weak upgrades.
    fn kin(&self, parents: bool) -> Vec<String> {
        let Some(cell) = self.sel_cell() else { return vec![] };
        let Some(node) = self.nodes.get(&cell.hash) else { return vec![] };
        let node = node.borrow();
        let refs = if parents { node.parents.borrow() } else { node.children.borrow() };
        refs.iter()
            .filter_map(|w| w.upgrade())
            .map(|n| n.borrow().hash.clone())
            .collect()
    }

    fn select_hash(&mut self, hash: &str) {
        if let Some(i) = self.cells.iter().position(|c| c.hash == hash) {
            self.sel = i;
        }
    }

    /// j (down = first parent) / k (up = first child).
    fn move_kin(&mut self, parents: bool) {
        if let Some(h) = self.kin(parents).first().cloned() {
            self.select_hash(&h);
        }
    }

    /// h/l: prefer a parent/child on a lane in that direction (lets you pick
    /// among merge parents / fork children), else nearest node on the nearest
    /// lane in that direction.
    fn move_lateral(&mut self, dir: isize) {
        let Some(cur) = self.sel_cell() else { return };
        let (cl, cr) = (cur.lane, cur.row);
        let mut kin = self.kin(true);
        kin.extend(self.kin(false));
        let kin_cells: Vec<(usize, usize)> = kin
            .iter()
            .filter_map(|h| self.pos.get(h).copied())
            .collect();
        let all: Vec<(usize, usize)> = self.cells.iter().map(|c| (c.lane, c.row)).collect();
        let target = nearest_lateral(&kin_cells, cl, cr, dir)
            .or_else(|| nearest_lateral(&all, cl, cr, dir));
        if let Some((l, r)) = target {
            if let Some(i) = self.cells.iter().position(|c| c.lane == l && c.row == r) {
                self.sel = i;
            }
        }
    }

    fn draw(&mut self, f: &mut ratatui::Frame) {
        let [title, status, graph, hints] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .areas(f.area());

        f.render_widget(
            Paragraph::new("⎇ VETO — commit constellation")
                .centered()
                .style(Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            title,
        );

        let status_line = match self.sel_cell().and_then(|c| self.nodes.get(&c.hash)) {
            Some(n) => {
                let n = n.borrow();
                format!(
                    " {} — {}",
                    n.short_hash,
                    n.message.lines().next().unwrap_or("")
                )
            }
            None => " no commits".into(),
        };
        f.render_widget(
            Paragraph::new(status_line).style(Style::new().fg(Color::Gray)),
            status,
        );

        self.draw_graph(f.buffer_mut(), graph);

        f.render_widget(
            Paragraph::new(" j/k parent/child   h/l lanes   i info   q quit")
                .style(Style::new().fg(Color::DarkGray)),
            hints,
        );

        if self.popup {
            self.draw_popup(f);
        }
    }

    fn draw_graph(&mut self, buf: &mut Buffer, area: Rect) {
        // keep selection visible
        if let Some(cell) = self.sel_cell() {
            let (sx, sy) = (cell.lane * LANE_W, cell.row * ROW_H);
            let (w, h) = (area.width as usize, area.height as usize);
            if w > 0 && h > 0 {
                self.scroll.0 = self.scroll.0.min(sx).max((sx + LANE_W).saturating_sub(w));
                self.scroll.1 = self.scroll.1.min(sy).max((sy + 1).saturating_sub(h));
            }
        }
        let (ox, oy) = self.scroll;
        let put = |buf: &mut Buffer, wx: usize, wy: usize, sym: &str, style: Style| {
            if wx < ox || wy < oy {
                return;
            }
            let (x, y) = (wx - ox, wy - oy);
            if x < area.width as usize && y < area.height as usize {
                if let Some(c) = buf.cell_mut((area.x + x as u16, area.y + y as u16)) {
                    c.set_symbol(sym).set_style(style);
                }
            }
        };

        // starfield: fixed pseudo-random world dots, twinkling with the tick
        for y in oy..oy + area.height as usize {
            for x in ox..ox + area.width as usize {
                let h = (x as u64)
                    .wrapping_mul(2654435761)
                    .wrapping_add((y as u64).wrapping_mul(0x9E3779B97F4A7C15));
                let h = h ^ (h >> 13);
                if h % 29 == 0 {
                    let bright = (h / 29 + self.tick as u64) % 8 < 2;
                    put(
                        buf,
                        x,
                        y,
                        if h % 173 == 0 { "✦" } else { "·" },
                        Style::new().fg(if bright { Color::Gray } else { Color::DarkGray }),
                    );
                }
            }
        }

        // edges child -> parent (drawn under nodes)
        for cell in &self.cells {
            let Some(node) = self.nodes.get(&cell.hash) else { continue };
            let parents: Vec<String> = node
                .borrow()
                .parents
                .borrow()
                .iter()
                .filter_map(|w| w.upgrade())
                .map(|n| n.borrow().hash.clone())
                .collect();
            let (cx, cy) = (cell.lane * LANE_W, cell.row * ROW_H);
            for ph in parents {
                let Some(&(pl, pr)) = self.pos.get(&ph) else { continue };
                let (px, py) = (pl * LANE_W, pr * ROW_H);
                if py <= cy {
                    continue; // ponytail: skip upward edges, layout puts parents below
                }
                let style = Style::new().fg(lane_color(pl));
                let turn = py - 1;
                for y in cy + 1..turn {
                    put(buf, cx, y, "│", style);
                }
                if px == cx {
                    put(buf, cx, turn, "│", style);
                } else {
                    put(buf, cx, turn, if px > cx { "╰" } else { "╯" }, style);
                    for x in cx.min(px) + 1..cx.max(px) {
                        put(buf, x, turn, "─", style);
                    }
                    put(buf, px, turn, if px > cx { "╮" } else { "╭" }, style);
                }
            }
        }

        // nodes
        for (i, cell) in self.cells.iter().enumerate() {
            let (x, y) = (cell.lane * LANE_W, cell.row * ROW_H);
            let selected = i == self.sel;
            let color = if selected {
                PULSE[self.tick % PULSE.len()] // pulse through the color cycle
            } else {
                lane_color(cell.lane)
            };
            put(buf, x, y, if selected { "◉" } else { "●" }, Style::new().fg(color));
            let short: String = cell.hash.chars().take(7).collect();
            let mut label = Style::new().fg(color);
            if selected {
                label = label.add_modifier(Modifier::BOLD);
            } else {
                label = label.add_modifier(Modifier::DIM);
            }
            for (j, ch) in short.chars().enumerate() {
                put(buf, x + 2 + j, y, &ch.to_string(), label);
            }
        }
    }

    fn draw_popup(&self, f: &mut ratatui::Frame) {
        let Some(cell) = self.sel_cell() else { return };
        let Some(node) = self.nodes.get(&cell.hash) else { return };
        let n = node.borrow();
        let join = |v: Vec<String>| if v.is_empty() { "—".into() } else { v.join(", ") };
        let parents = join(
            n.parents
                .borrow()
                .iter()
                .filter_map(|w| w.upgrade())
                .map(|p| p.borrow().short_hash.clone())
                .collect(),
        );
        let children = join(
            n.children
                .borrow()
                .iter()
                .filter_map(|w| w.upgrade())
                .map(|c| c.borrow().short_hash.clone())
                .collect(),
        );
        let lines = vec![
            Line::from(format!("hash     {}", n.hash)),
            Line::from(format!("short    {}", n.short_hash)),
            Line::from(format!("date     {} ({})", fmt_epoch(n.timestamp), ago(n.timestamp))),
            Line::from(format!("message  {}", n.message)),
            Line::from(format!("parents  {parents}")),
            Line::from(format!("children {children}")),
        ];
        let area = f.area();
        let w = 72.min(area.width.saturating_sub(4)).max(20);
        let h = (lines.len() as u16 + 2).min(area.height);
        let rect = Rect::new(
            area.x + (area.width.saturating_sub(w)) / 2,
            area.y + (area.height.saturating_sub(h)) / 2,
            w,
            h,
        );
        f.render_widget(Clear, rect);
        f.render_widget(
            Paragraph::new(lines)
                .wrap(Wrap { trim: false })
                .block(
                    Block::bordered()
                        .title(format!(" ◉ {} ", n.short_hash))
                        .border_style(Style::new().fg(Color::Cyan)),
                ),
            rect,
        );
    }
}

/// Nearest (lane,row) among `cands` strictly in lane direction `dir` from
/// (lane,row); ties broken by lane distance then row distance.
fn nearest_lateral(
    cands: &[(usize, usize)],
    lane: usize,
    row: usize,
    dir: isize,
) -> Option<(usize, usize)> {
    cands
        .iter()
        .filter(|(l, _)| (*l as isize - lane as isize).signum() == dir)
        .min_by_key(|(l, r)| (lane.abs_diff(*l), row.abs_diff(*r)))
        .copied()
}

/// Unix epoch seconds -> "YYYY-MM-DD HH:MM:SS UTC" (Howard Hinnant's civil_from_days).
fn fmt_epoch(ts: u64) -> String {
    let z = (ts / 86400) as i64 + 719468;
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + (m <= 2) as i64;
    let s = ts % 86400;
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02} UTC",
        s / 3600,
        (s % 3600) / 60,
        s % 60
    )
}

fn ago(ts: u64) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(ts);
    let days = now.saturating_sub(ts) / 86400;
    match days {
        0 => "today".into(),
        1 => "1 day ago".into(),
        n => format!("{n} days ago"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fmt_epoch_known_dates() {
        assert_eq!(fmt_epoch(0), "1970-01-01 00:00:00 UTC");
        assert_eq!(fmt_epoch(1_000_000_000), "2001-09-09 01:46:40 UTC");
        assert_eq!(fmt_epoch(1_752_278_400), "2025-07-12 00:00:00 UTC");
    }

    #[test]
    fn nearest_lateral_picks_closest_lane_then_row() {
        let cells = [(0, 5), (2, 1), (1, 9), (1, 4)];
        assert_eq!(nearest_lateral(&cells, 1, 5, -1), Some((0, 5)));
        assert_eq!(nearest_lateral(&cells, 0, 5, 1), Some((1, 4)));
        assert_eq!(nearest_lateral(&cells, 2, 1, 1), None);
    }
}
