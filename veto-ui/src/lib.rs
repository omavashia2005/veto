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
    crossterm::{
        event::{self, Event, KeyCode, KeyEventKind},
        execute,
        terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
    },
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    symbols::Marker,
    text::Line,
    widgets::{
        Block, Clear, Paragraph, Wrap,
        canvas::{Canvas, Line as Line3},
    },
};
use veto_graph::CommitNode;

pub mod layout;
use layout::Cell;

type Nodes = HashMap<String, Rc<RefCell<CommitNode>>>;

/// zoom levels: (view radius in world units, draw hash labels)
const ZOOM: [(f64, bool); 4] = [(90.0, false), (45.0, false), (22.0, true), (12.0, true)];
const SPACING_T: f64 = 6.0; // world units per commit along the time axis
const SPACING_L: f64 = 4.0; // per lane
const FOCAL: f64 = 60.0; // perspective focal length
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

/// Stable per-hash depth so the constellation isn't a flat sheet.
fn jitter(hash: &str) -> f64 {
    let h = hash
        .bytes()
        .fold(0u64, |a, b| a.wrapping_mul(31).wrapping_add(b as u64));
    (h % 9) as f64 - 4.0
}

/// Rotate (a, b) by ang radians.
fn rot(a: f64, b: f64, ang: f64) -> (f64, f64) {
    (a * ang.cos() - b * ang.sin(), a * ang.sin() + b * ang.cos())
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
    branches: &'a [(String, String)],     // (name, tip hash)
    sel: usize,                           // index into cells
    popup: bool,
    tick: usize,
    zoom: usize, // index into ZOOM
    yaw: f64,
    pitch: f64,
    idle: usize, // ticks since last keypress, drives the screensaver spin
}

/// Take over the terminal and run the interactive 3D commit-graph explorer.
/// Time flows left (old) to right (new): h/l walk parent/child, j/k move
/// across lanes, b jumps between branch tips, arrows rotate the view,
/// +/- zoom, i shows metadata, q quits.
pub fn run(nodes: &Nodes, branches: &[(String, String)]) -> io::Result<()> {
    let cells = layout::layout(nodes);
    let pos = cells
        .iter()
        .map(|c| (c.hash.clone(), (c.lane, c.row)))
        .collect();
    let mut app = App {
        nodes,
        cells,
        pos,
        branches,
        sel: 0, // cells sorted by row asc, so index 0 is a row-0 (newest) cell
        popup: false,
        tick: 0,
        zoom: 2,
        yaw: 0.35, // start slightly tilted so the depth reads immediately
        pitch: 0.25,
        idle: 0,
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
                app.idle = 0;
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    KeyCode::Char('i') => app.popup = !app.popup,
                    _ if app.popup => app.popup = false, // any key closes the popup
                    KeyCode::Char('h') => app.move_kin(true), // left = older = parent
                    KeyCode::Char('l') => app.move_kin(false), // right = newer = child
                    KeyCode::Char('j') => app.move_lateral(1),
                    KeyCode::Char('k') => app.move_lateral(-1),
                    KeyCode::Char('b') => app.next_branch(),
                    KeyCode::Left => app.yaw -= 0.15,
                    KeyCode::Right => app.yaw += 0.15,
                    KeyCode::Up => app.pitch = (app.pitch + 0.1).min(1.4),
                    KeyCode::Down => app.pitch = (app.pitch - 0.1).max(-1.4),
                    KeyCode::Char('+') | KeyCode::Char('=') => {
                        app.zoom = (app.zoom + 1).min(ZOOM.len() - 1)
                    }
                    KeyCode::Char('-') => app.zoom = app.zoom.saturating_sub(1),
                    _ => {}
                }
            }
        } else {
            app.tick = app.tick.wrapping_add(1);
            app.idle += 1;
            if app.idle > 50 {
                app.yaw += 0.01; // idle screensaver: slow orbit
            }
        }
    }
    Ok(())
}

impl App<'_> {
    fn sel_cell(&self) -> Option<&Cell> {
        self.cells.get(self.sel)
    }

    /// Branch name if the given hash is a branch tip.
    fn branch_of(&self, hash: &str) -> Option<&str> {
        self.branches
            .iter()
            .find(|(_, tip)| tip == hash)
            .map(|(name, _)| name.as_str())
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

    /// h (left = first parent, older) / l (right = first child, newer).
    fn move_kin(&mut self, parents: bool) {
        if let Some(h) = self.kin(parents).first().cloned() {
            self.select_hash(&h);
        }
    }

    /// b: jump to the next branch tip (wraps around).
    fn next_branch(&mut self) {
        if self.branches.is_empty() {
            return;
        }
        let cur = self.sel_cell().map(|c| c.hash.clone());
        let next = match self
            .branches
            .iter()
            .position(|(_, tip)| Some(tip) == cur.as_ref())
        {
            Some(i) => (i + 1) % self.branches.len(),
            None => 0,
        };
        let hash = self.branches[next].1.clone();
        self.select_hash(&hash);
    }

    /// j/k: prefer a parent/child on a lane in that direction (lets you pick
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

    /// World position: x = time axis (right = newer), y = lane, z = hash depth.
    fn world3(&self, lane: usize, row: usize, hash: &str) -> (f64, f64, f64) {
        let max_row = self.cells.len().saturating_sub(1); // one commit per row
        (
            (max_row - row) as f64 * SPACING_T,
            lane as f64 * SPACING_L,
            jitter(hash),
        )
    }

    /// Perspective-project into view space centered on the selection (which
    /// therefore always sits at the middle of the screen and is the pivot the
    /// view rotates around). Returns (screen x, screen y, depth).
    fn project(&self, lane: usize, row: usize, hash: &str) -> (f64, f64, f64) {
        let c = self
            .sel_cell()
            .map(|c| self.world3(c.lane, c.row, &c.hash))
            .unwrap_or((0.0, 0.0, 0.0));
        let w = self.world3(lane, row, hash);
        let (mut x, mut y, mut z) = (w.0 - c.0, w.1 - c.1, w.2 - c.2);
        (x, z) = rot(x, z, self.yaw);
        (y, z) = rot(y, z, self.pitch);
        // ponytail: clamp instead of culling points behind the camera
        let s = FOCAL / (FOCAL + z).max(1.0);
        (x * s, -y * s, z) // negate y: lane 0 renders at the top
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
                let branch = self
                    .branch_of(&n.hash)
                    .map(|b| format!(" [{b}]"))
                    .unwrap_or_default();
                format!(
                    " {}{branch} — {}",
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

        self.draw_graph(f, graph);

        f.render_widget(
            Paragraph::new(" h/l parent/child  j/k lanes  b branch  ←↑↓→ rotate  +/- zoom  i info  q quit")
                .style(Style::new().fg(Color::DarkGray)),
            hints,
        );

        if self.popup {
            self.draw_popup(f);
        }
    }

    fn draw_graph(&self, f: &mut ratatui::Frame, area: Rect) {
        let (radius, labels) = ZOOM[self.zoom];
        // terminal cells are ~2:1, keep the world roughly isotropic
        let yr = radius * area.height.max(1) as f64 * 2.0 / area.width.max(1) as f64;
        let row_h = yr * 2.0 / area.height.max(1) as f64; // one text row in canvas units

        let canvas = Canvas::default()
            .x_bounds([-radius, radius])
            .y_bounds([-yr, yr])
            .marker(Marker::Braille)
            .paint(|ctx| {
                // starfield drifting opposite the rotation for parallax
                for i in 0..80u64 {
                    let h = i
                        .wrapping_mul(0x9E3779B97F4A7C15)
                        .wrapping_add(i << 7);
                    let bx = (h % 997) as f64 / 997.0 * 2.0 - 1.0;
                    let by = (h / 997 % 991) as f64 / 991.0 * 2.0 - 1.0;
                    let x = (bx * radius - self.yaw * radius * 0.3).rem_euclid(2.0 * radius) - radius;
                    let y = (by * yr + self.pitch * yr * 0.3).rem_euclid(2.0 * yr) - yr;
                    let bright = (h / 31 + self.tick as u64) % 8 < 2;
                    ctx.print(
                        x,
                        y,
                        Line::styled(
                            if h % 13 == 0 { "✦" } else { "·" },
                            Style::new().fg(if bright { Color::Gray } else { Color::DarkGray }),
                        ),
                    );
                }

                // edges child -> parent
                for cell in &self.cells {
                    let Some(node) = self.nodes.get(&cell.hash) else { continue };
                    let (x1, y1, _) = self.project(cell.lane, cell.row, &cell.hash);
                    for w in node.borrow().parents.borrow().iter() {
                        let Some(p) = w.upgrade() else { continue };
                        let ph = p.borrow().hash.clone();
                        let Some(&(pl, pr)) = self.pos.get(&ph) else { continue };
                        let (x2, y2, _) = self.project(pl, pr, &ph);
                        ctx.draw(&Line3 {
                            x1,
                            y1,
                            x2,
                            y2,
                            color: lane_color(pl),
                        });
                    }
                }
                ctx.layer(); // nodes render above the edge lines

                // nodes, painter's order: far ones first so near ones win
                let mut order: Vec<(usize, &Cell, (f64, f64, f64))> = self
                    .cells
                    .iter()
                    .enumerate()
                    .map(|(i, c)| (i, c, self.project(c.lane, c.row, &c.hash)))
                    .collect();
                order.sort_by(|a, b| b.2 .2.total_cmp(&a.2 .2));

                for (i, cell, (x, y, z)) in order {
                    let selected = i == self.sel;
                    let far = z > FOCAL * 0.5;
                    let color = if selected {
                        PULSE[self.tick % PULSE.len()] // pulse through the color cycle
                    } else if far {
                        Color::DarkGray // depth cue: distant commits fade out
                    } else {
                        lane_color(cell.lane)
                    };
                    let glyph = if selected { "◉" } else if far { "·" } else { "●" };
                    let text = if labels && !far {
                        let short: String = cell.hash.chars().take(7).collect();
                        format!("{glyph} {short}")
                    } else {
                        glyph.to_string()
                    };
                    let mut style = Style::new().fg(color);
                    if selected {
                        style = style.add_modifier(Modifier::BOLD);
                    }
                    ctx.print(x, y, Line::styled(text, style));

                    if let Some(name) = self.branch_of(&cell.hash) {
                        ctx.print(
                            x,
                            y - row_h,
                            Line::styled(
                                format!("[{name}]"),
                                Style::new().fg(Color::LightMagenta).add_modifier(Modifier::BOLD),
                            ),
                        );
                    }
                }
            });
        f.render_widget(canvas, area);
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
        let mut lines = vec![
            Line::from(format!("hash     {}", n.hash)),
            Line::from(format!("short    {}", n.short_hash)),
            Line::from(format!("date     {} ({})", fmt_epoch(n.timestamp), ago(n.timestamp))),
            Line::from(format!("message  {}", n.message)),
            Line::from(format!("parents  {parents}")),
            Line::from(format!("children {children}")),
        ];
        if let Some(branch) = self.branch_of(&n.hash) {
            lines.push(Line::from(format!("branch   {branch}")));
        }
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

    #[test]
    fn rot_quarter_turn_maps_x_onto_z() {
        let (x, z) = rot(1.0, 0.0, std::f64::consts::FRAC_PI_2);
        assert!(x.abs() < 1e-9);
        assert!((z - 1.0).abs() < 1e-9);
    }
}
