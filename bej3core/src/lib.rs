//! Match-3 game logic. No graphics, no I/O.
//!
//! The rules implemented here are the ones described by the project owner:
//!
//! * A line of 3 or more is cleared.
//! * A line of 4 creates a flame gem. When it is part of a match it clears the
//!   3x3 area around itself.
//! * A line of 5 creates a hypercube. Swapped with any gem it clears every gem
//!   of that gem's color. Two hypercubes swapped together clear the board.
//! * Two crossing groups of 3 or more (L, T or +) create a star gem. When it
//!   is part of a match it clears its whole row and column.
//! * A straight line of 6 or more (see `Rules::supernova_min`) creates a
//!   supernova gem. When it is part of a match it clears three rows and three
//!   columns centered on it.
//!
//! If one match qualifies for several specials the priority is supernova,
//! then hypercube, then star, then flame. A special gem caught in another
//! special's blast triggers in turn. A hypercube caught in a blast is simply
//! cleared (open question, see NOTES.md).
//!
//! `try_swap` never draws anything. It changes the board and returns a list of
//! `Event`s that a renderer can play back, which also makes replays and tests
//! easy: same seed plus same moves gives the same events.

use std::collections::BTreeMap;
use std::fmt;

pub const W: usize = 8;
pub const H: usize = 8;

/// (row, column). Row 0 is the top of the board.
pub type Pos = (usize, usize);

const HYPERCUBE_LEN: usize = 5;
const FLAME_LEN: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Color {
    Red,
    Orange,
    Yellow,
    Green,
    Blue,
    Purple,
    White,
}

pub const ALL_COLORS: [Color; 7] = [
    Color::Red,
    Color::Orange,
    Color::Yellow,
    Color::Green,
    Color::Blue,
    Color::Purple,
    Color::White,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Special {
    None,
    Flame,
    Star,
    Supernova,
    Hypercube,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Gem {
    pub color: Color,
    pub special: Special,
}

impl Gem {
    pub fn plain(color: Color) -> Gem {
        Gem {
            color,
            special: Special::None,
        }
    }

    /// Hypercubes never take part in color matches.
    fn matchable(&self) -> bool {
        self.special != Special::Hypercube
    }
}

fn color_char(c: Color) -> char {
    match c {
        Color::Red => 'R',
        Color::Orange => 'O',
        Color::Yellow => 'Y',
        Color::Green => 'G',
        Color::Blue => 'B',
        Color::Purple => 'P',
        Color::White => 'W',
    }
}

fn color_from_char(ch: char) -> Option<Color> {
    match ch {
        'R' => Some(Color::Red),
        'O' => Some(Color::Orange),
        'Y' => Some(Color::Yellow),
        'G' => Some(Color::Green),
        'B' => Some(Color::Blue),
        'P' => Some(Color::Purple),
        'W' => Some(Color::White),
        _ => None,
    }
}

/// Settings that sandbox mode can expose later.
#[derive(Clone, Debug)]
pub struct Rules {
    /// Number of gem colors in play, 3 to 7.
    pub colors: usize,
    /// Straight line length that creates a supernova gem.
    pub supernova_min: usize,
}

impl Default for Rules {
    fn default() -> Self {
        Rules {
            colors: 7,
            supernova_min: 6,
        }
    }
}

/// Small seeded generator (xorshift64*), so games are reproducible.
#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    pub fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n as u64) as usize
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Board {
    pub cells: [[Option<Gem>; W]; H],
}

impl Board {
    pub fn empty() -> Board {
        Board {
            cells: [[None; W]; H],
        }
    }

    /// Build a board from 8 strings of 8 characters: `R O Y G B P W` for plain
    /// gems, anything else for an empty cell. Handy for tests.
    pub fn from_rows(rows: &[&str]) -> Board {
        assert_eq!(rows.len(), H, "need {H} rows");
        let mut b = Board::empty();
        for (r, line) in rows.iter().enumerate() {
            let chars: Vec<char> = line.chars().collect();
            assert_eq!(chars.len(), W, "row {r} needs {W} cells");
            for (c, ch) in chars.into_iter().enumerate() {
                b.cells[r][c] = color_from_char(ch).map(Gem::plain);
            }
        }
        b
    }

    /// A fresh board with no matches on it and at least one legal move.
    pub fn random(rng: &mut Rng, rules: &Rules) -> Board {
        assert!((3..=7).contains(&rules.colors), "rules.colors must be 3..=7");
        loop {
            let mut b = Board::empty();
            for r in 0..H {
                for c in 0..W {
                    loop {
                        let color = ALL_COLORS[rng.below(rules.colors)];
                        let left = c >= 2
                            && b.color_at(r, c - 1) == Some(color)
                            && b.color_at(r, c - 2) == Some(color);
                        let up = r >= 2
                            && b.color_at(r - 1, c) == Some(color)
                            && b.color_at(r - 2, c) == Some(color);
                        if !left && !up {
                            b.cells[r][c] = Some(Gem::plain(color));
                            break;
                        }
                    }
                }
            }
            if has_move(&b, rules) {
                return b;
            }
        }
    }

    pub fn get(&self, p: Pos) -> Option<Gem> {
        self.cells[p.0][p.1]
    }

    pub fn set(&mut self, p: Pos, g: Option<Gem>) {
        self.cells[p.0][p.1] = g;
    }

    pub fn swap(&mut self, a: Pos, b: Pos) {
        let t = self.cells[a.0][a.1];
        self.cells[a.0][a.1] = self.cells[b.0][b.1];
        self.cells[b.0][b.1] = t;
    }

    /// Color used for matching. `None` for empty cells and hypercubes.
    fn color_at(&self, r: usize, c: usize) -> Option<Color> {
        self.cells[r][c].filter(Gem::matchable).map(|g| g.color)
    }

    /// Text picture of the board. Two characters per cell: a color letter and
    /// a marker (`*` flame, `+` star, `#` supernova). Hypercubes show as `@@`.
    pub fn render(&self) -> String {
        let mut s = String::from("    ");
        for c in 0..W {
            s.push_str(&format!(" {c} "));
        }
        s.push('\n');
        for r in 0..H {
            s.push_str(&format!(" {r}  "));
            for c in 0..W {
                let cell = match self.cells[r][c] {
                    None => "..".to_string(),
                    Some(g) => match g.special {
                        Special::Hypercube => "@@".to_string(),
                        sp => {
                            let marker = match sp {
                                Special::Flame => '*',
                                Special::Star => '+',
                                Special::Supernova => '#',
                                _ => ' ',
                            };
                            format!("{}{}", color_char(g.color), marker)
                        }
                    },
                };
                s.push_str(&cell);
                s.push(' ');
            }
            s.push('\n');
        }
        s
    }
}

/// Things that happened, in order, for a renderer to play back.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Swap { a: Pos, b: Pos },
    /// These gems were removed.
    Clear { cells: Vec<Pos> },
    /// A special gem was created.
    Spawn { pos: Pos, gem: Gem },
    /// A special gem's effect fired (for visuals).
    Trigger { pos: Pos, special: Special },
    /// Gems fell from the first position to the second.
    Fall { moves: Vec<(Pos, Pos)> },
    /// New gems entered from the top.
    Fill { gems: Vec<(Pos, Gem)> },
    /// No legal move remains.
    NoMoves,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwapError {
    OutOfBounds,
    NotAdjacent,
    EmptyCell,
    NoMatch,
    GameOver,
}

impl fmt::Display for SwapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            SwapError::OutOfBounds => "position is off the board",
            SwapError::NotAdjacent => "those cells are not next to each other",
            SwapError::EmptyCell => "one of those cells is empty",
            SwapError::NoMatch => "that swap makes no match",
            SwapError::GameOver => "the game is over",
        };
        write!(f, "{s}")
    }
}

impl std::error::Error for SwapError {}

/// A straight line of 3 or more same-colored gems.
struct Run {
    cells: Vec<Pos>,
    horizontal: bool,
}

fn find_runs(bd: &Board) -> Vec<Run> {
    let mut runs = Vec::new();
    for r in 0..H {
        let mut c = 0;
        while c < W {
            let col = bd.color_at(r, c);
            let mut end = c + 1;
            if col.is_some() {
                while end < W && bd.color_at(r, end) == col {
                    end += 1;
                }
            }
            if col.is_some() && end - c >= 3 {
                runs.push(Run {
                    cells: (c..end).map(|x| (r, x)).collect(),
                    horizontal: true,
                });
            }
            c = end;
        }
    }
    for c in 0..W {
        let mut r = 0;
        while r < H {
            let col = bd.color_at(r, c);
            let mut end = r + 1;
            if col.is_some() {
                while end < H && bd.color_at(end, c) == col {
                    end += 1;
                }
            }
            if col.is_some() && end - r >= 3 {
                runs.push(Run {
                    cells: (r..end).map(|y| (y, c)).collect(),
                    horizontal: false,
                });
            }
            r = end;
        }
    }
    runs
}

/// A set of connected runs that clear together, and what they create.
#[derive(Debug, Clone)]
pub struct Group {
    pub color: Color,
    pub cells: Vec<Pos>,
    pub create: Special,
    pub spawn_at: Pos,
}

fn root(parent: &mut [usize], mut x: usize) -> usize {
    while parent[x] != x {
        parent[x] = parent[parent[x]];
        x = parent[x];
    }
    x
}

/// A cell that lies on both a horizontal and a vertical run of the group.
fn intersection(runs: &[Run], idxs: &[usize]) -> Option<Pos> {
    for &i in idxs {
        if !runs[i].horizontal {
            continue;
        }
        for &j in idxs {
            if runs[j].horizontal {
                continue;
            }
            if let Some(&c) = runs[i].cells.iter().find(|c| runs[j].cells.contains(c)) {
                return Some(c);
            }
        }
    }
    None
}

/// Find every match on the board. `swapped` lists the two cells the player
/// just swapped (empty during cascades); a special gem is created on one of
/// them when it belongs to the match.
pub fn find_groups(bd: &Board, rules: &Rules, swapped: &[Pos]) -> Vec<Group> {
    let runs = find_runs(bd);
    let n = runs.len();

    // Join runs that share a cell.
    let mut parent: Vec<usize> = (0..n).collect();
    for i in 0..n {
        for j in (i + 1)..n {
            let shares = runs[i].cells.iter().any(|c| runs[j].cells.contains(c));
            if shares {
                let ri = root(&mut parent, i);
                let rj = root(&mut parent, j);
                if ri != rj {
                    parent[ri] = rj;
                }
            }
        }
    }
    let mut by_root: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for i in 0..n {
        let r = root(&mut parent, i);
        by_root.entry(r).or_default().push(i);
    }

    let mut groups = Vec::new();
    for idxs in by_root.values() {
        let mut cells: Vec<Pos> = Vec::new();
        for &i in idxs {
            for &c in &runs[i].cells {
                if !cells.contains(&c) {
                    cells.push(c);
                }
            }
        }
        let color = bd
            .color_at(cells[0].0, cells[0].1)
            .expect("a run cell always has a color");
        let longest = idxs
            .iter()
            .map(|&i| &runs[i])
            .max_by_key(|r| r.cells.len())
            .expect("a group has at least one run");
        let has_h = idxs.iter().any(|&i| runs[i].horizontal);
        let has_v = idxs.iter().any(|&i| !runs[i].horizontal);
        let len = longest.cells.len();

        let create = if len >= rules.supernova_min {
            Special::Supernova
        } else if len >= HYPERCUBE_LEN {
            Special::Hypercube
        } else if has_h && has_v {
            Special::Star
        } else if len >= FLAME_LEN {
            Special::Flame
        } else {
            Special::None
        };

        let middle = longest.cells[longest.cells.len() / 2];
        let spawn_at = if create == Special::None {
            cells[0]
        } else if let Some(&p) = swapped.iter().find(|p| cells.contains(p)) {
            p
        } else if create == Special::Star {
            intersection(&runs, idxs).unwrap_or(middle)
        } else {
            middle
        };

        groups.push(Group {
            color,
            cells,
            create,
            spawn_at,
        });
    }
    groups
}

/// Cells hit by a special gem at `p`, clipped to the board.
fn effect_cells(special: Special, p: Pos) -> Vec<Pos> {
    let mut out = Vec::new();
    match special {
        Special::Flame => {
            for r in 0..H {
                for c in 0..W {
                    if r.abs_diff(p.0) <= 1 && c.abs_diff(p.1) <= 1 {
                        out.push((r, c));
                    }
                }
            }
        }
        Special::Star => {
            for c in 0..W {
                out.push((p.0, c));
            }
            for r in 0..H {
                if r != p.0 {
                    out.push((r, p.1));
                }
            }
        }
        Special::Supernova => {
            for r in 0..H {
                for c in 0..W {
                    if r.abs_diff(p.0) <= 1 || c.abs_diff(p.1) <= 1 {
                        out.push((r, c));
                    }
                }
            }
        }
        Special::None | Special::Hypercube => {}
    }
    out
}

/// Grow a set of cleared cells by firing every flame, star and supernova gem
/// caught in it. Returns all cells to clear, in discovery order.
fn expand_clears(bd: &Board, start: &[Pos], events: &mut Vec<Event>) -> Vec<Pos> {
    let mut marked = [[false; W]; H];
    let mut queue: Vec<Pos> = Vec::new();
    for &p in start {
        if !marked[p.0][p.1] {
            marked[p.0][p.1] = true;
            queue.push(p);
        }
    }
    let mut i = 0;
    while i < queue.len() {
        let p = queue[i];
        i += 1;
        if let Some(g) = bd.get(p) {
            if matches!(g.special, Special::Flame | Special::Star | Special::Supernova) {
                events.push(Event::Trigger {
                    pos: p,
                    special: g.special,
                });
                for q in effect_cells(g.special, p) {
                    if !marked[q.0][q.1] && bd.get(q).is_some() {
                        marked[q.0][q.1] = true;
                        queue.push(q);
                    }
                }
            }
        }
    }
    queue
}

fn gravity(bd: &mut Board, events: &mut Vec<Event>) {
    let mut moves: Vec<(Pos, Pos)> = Vec::new();
    for c in 0..W {
        let mut dest = H as isize - 1;
        for r in (0..H).rev() {
            if let Some(g) = bd.cells[r][c] {
                if r as isize != dest {
                    bd.cells[dest as usize][c] = Some(g);
                    bd.cells[r][c] = None;
                    moves.push(((r, c), (dest as usize, c)));
                }
                dest -= 1;
            }
        }
    }
    if !moves.is_empty() {
        events.push(Event::Fall { moves });
    }
}

fn fill(bd: &mut Board, rng: &mut Rng, rules: &Rules, events: &mut Vec<Event>) {
    let mut gems: Vec<(Pos, Gem)> = Vec::new();
    for c in 0..W {
        for r in 0..H {
            if bd.cells[r][c].is_none() {
                let g = Gem::plain(ALL_COLORS[rng.below(rules.colors)]);
                bd.cells[r][c] = Some(g);
                gems.push(((r, c), g));
            }
        }
    }
    if !gems.is_empty() {
        events.push(Event::Fill { gems });
    }
}

/// Clear, spawn, fall and refill, repeating while new matches appear.
/// `extra` are cells to clear in the first round besides the groups' cells
/// (used by hypercube swaps).
fn run_cascade(
    bd: &mut Board,
    rng: &mut Rng,
    rules: &Rules,
    first_groups: Vec<Group>,
    extra: Vec<Pos>,
    events: &mut Vec<Event>,
) {
    let mut groups = first_groups;
    let mut pending_extra = extra;
    loop {
        let mut start: Vec<Pos> = pending_extra.drain(..).collect();
        for g in &groups {
            start.extend(g.cells.iter().copied());
        }
        let cleared = expand_clears(bd, &start, events);
        events.push(Event::Clear {
            cells: cleared.clone(),
        });
        for &p in &cleared {
            bd.set(p, None);
        }
        for g in &groups {
            if g.create != Special::None {
                let gem = Gem {
                    color: g.color,
                    special: g.create,
                };
                bd.set(g.spawn_at, Some(gem));
                events.push(Event::Spawn {
                    pos: g.spawn_at,
                    gem,
                });
            }
        }
        gravity(bd, events);
        fill(bd, rng, rules, events);

        groups = find_groups(bd, rules, &[]);
        if groups.is_empty() {
            break;
        }
    }
}

/// Swap two adjacent cells and resolve everything that follows.
/// On `Err` the board is unchanged.
pub fn try_swap(
    bd: &mut Board,
    rng: &mut Rng,
    rules: &Rules,
    a: Pos,
    b: Pos,
) -> Result<Vec<Event>, SwapError> {
    if a.0 >= H || a.1 >= W || b.0 >= H || b.1 >= W {
        return Err(SwapError::OutOfBounds);
    }
    if a.0.abs_diff(b.0) + a.1.abs_diff(b.1) != 1 {
        return Err(SwapError::NotAdjacent);
    }
    let (ga, gb) = match (bd.get(a), bd.get(b)) {
        (Some(x), Some(y)) => (x, y),
        _ => return Err(SwapError::EmptyCell),
    };

    let mut events = vec![Event::Swap { a, b }];
    let a_hyper = ga.special == Special::Hypercube;
    let b_hyper = gb.special == Special::Hypercube;

    if a_hyper || b_hyper {
        let mut start: Vec<Pos> = Vec::new();
        if a_hyper && b_hyper {
            for r in 0..H {
                for c in 0..W {
                    start.push((r, c));
                }
            }
        } else {
            let (hp, other) = if a_hyper { (a, gb) } else { (b, ga) };
            start.push(hp);
            for r in 0..H {
                for c in 0..W {
                    if let Some(g) = bd.cells[r][c] {
                        if g.special != Special::Hypercube && g.color == other.color {
                            start.push((r, c));
                        }
                    }
                }
            }
        }
        run_cascade(bd, rng, rules, Vec::new(), start, &mut events);
    } else {
        bd.swap(a, b);
        let groups = find_groups(bd, rules, &[a, b]);
        if groups.is_empty() {
            bd.swap(a, b);
            return Err(SwapError::NoMatch);
        }
        run_cascade(bd, rng, rules, groups, Vec::new(), &mut events);
    }
    Ok(events)
}

/// First legal swap found, if any. Used for hints and game-over detection.
pub fn find_move(bd: &Board, rules: &Rules) -> Option<(Pos, Pos)> {
    let mut t = bd.clone();
    for r in 0..H {
        for c in 0..W {
            for (dr, dc) in [(0usize, 1usize), (1, 0)] {
                let (r2, c2) = (r + dr, c + dc);
                if r2 >= H || c2 >= W {
                    continue;
                }
                let (a, b) = ((r, c), (r2, c2));
                let (ga, gb) = match (t.get(a), t.get(b)) {
                    (Some(x), Some(y)) => (x, y),
                    _ => continue,
                };
                if ga.special == Special::Hypercube || gb.special == Special::Hypercube {
                    return Some((a, b));
                }
                t.swap(a, b);
                let ok = !find_groups(&t, rules, &[a, b]).is_empty();
                t.swap(a, b);
                if ok {
                    return Some((a, b));
                }
            }
        }
    }
    None
}

pub fn has_move(bd: &Board, rules: &Rules) -> bool {
    find_move(bd, rules).is_some()
}

/// A game in progress. Scoring and level progress are not modelled yet.
pub struct Game {
    pub board: Board,
    pub rng: Rng,
    pub rules: Rules,
    pub gems_cleared: u64,
    pub moves: u32,
    pub game_over: bool,
}

impl Game {
    pub fn new(seed: u64, rules: Rules) -> Game {
        let mut rng = Rng::new(seed);
        let board = Board::random(&mut rng, &rules);
        Game {
            board,
            rng,
            rules,
            gems_cleared: 0,
            moves: 0,
            game_over: false,
        }
    }

    pub fn play(&mut self, a: Pos, b: Pos) -> Result<Vec<Event>, SwapError> {
        if self.game_over {
            return Err(SwapError::GameOver);
        }
        let mut events = try_swap(&mut self.board, &mut self.rng, &self.rules, a, b)?;
        self.moves += 1;
        for e in &events {
            if let Event::Clear { cells } = e {
                self.gems_cleared += cells.len() as u64;
            }
        }
        if !has_move(&self.board, &self.rules) {
            self.game_over = true;
            events.push(Event::NoMoves);
        }
        Ok(events)
    }

    pub fn hint(&self) -> Option<(Pos, Pos)> {
        find_move(&self.board, &self.rules)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A full board with no runs anywhere: color index is (2*row + col) % 7.
    fn base() -> Board {
        let mut b = Board::empty();
        for r in 0..H {
            for c in 0..W {
                b.cells[r][c] = Some(Gem::plain(ALL_COLORS[(2 * r + c) % 7]));
            }
        }
        b
    }

    fn red(b: &mut Board, cells: &[Pos]) {
        for &p in cells {
            b.set(p, Some(Gem::plain(Color::Red)));
        }
    }

    fn make_special(b: &mut Board, p: Pos, special: Special) {
        b.set(
            p,
            Some(Gem {
                color: Color::Red,
                special,
            }),
        );
    }

    fn cleared_by(b: &Board) -> usize {
        let groups = find_groups(b, &Rules::default(), &[]);
        let start: Vec<Pos> = groups
            .iter()
            .flat_map(|g| g.cells.iter().copied())
            .collect();
        let mut ev = Vec::new();
        expand_clears(b, &start, &mut ev).len()
    }

    #[test]
    fn base_board_has_no_matches() {
        assert!(find_groups(&base(), &Rules::default(), &[]).is_empty());
    }

    #[test]
    fn three_in_a_row_creates_nothing() {
        let mut b = base();
        red(&mut b, &[(0, 0), (0, 1), (0, 2)]);
        let g = find_groups(&b, &Rules::default(), &[]);
        assert_eq!(g.len(), 1);
        assert_eq!(g[0].create, Special::None);
        assert_eq!(g[0].cells.len(), 3);
    }

    #[test]
    fn four_in_a_row_creates_flame() {
        let mut b = base();
        red(&mut b, &[(0, 0), (0, 1), (0, 2), (0, 3)]);
        let g = find_groups(&b, &Rules::default(), &[]);
        assert_eq!(g.len(), 1);
        assert_eq!(g[0].create, Special::Flame);
    }

    #[test]
    fn five_in_a_row_creates_hypercube() {
        let mut b = base();
        red(&mut b, &[(0, 0), (0, 1), (0, 2), (0, 3), (0, 4)]);
        let g = find_groups(&b, &Rules::default(), &[]);
        assert_eq!(g[0].create, Special::Hypercube);
    }

    #[test]
    fn six_in_a_row_creates_supernova() {
        let mut b = base();
        red(&mut b, &[(0, 0), (0, 1), (0, 2), (0, 3), (0, 4), (0, 5)]);
        let g = find_groups(&b, &Rules::default(), &[]);
        assert_eq!(g[0].create, Special::Supernova);
    }

    #[test]
    fn crossing_groups_create_star_at_the_crossing() {
        let mut b = base();
        red(&mut b, &[(0, 0), (0, 1), (0, 2), (1, 0), (2, 0)]);
        let g = find_groups(&b, &Rules::default(), &[]);
        assert_eq!(g.len(), 1);
        assert_eq!(g[0].create, Special::Star);
        assert_eq!(g[0].spawn_at, (0, 0));
        assert_eq!(g[0].cells.len(), 5);
    }

    #[test]
    fn special_is_created_on_the_swapped_cell() {
        let mut b = base();
        red(&mut b, &[(0, 0), (0, 1), (0, 2), (0, 3)]);
        let g = find_groups(&b, &Rules::default(), &[(0, 3), (1, 3)]);
        assert_eq!(g[0].spawn_at, (0, 3));
    }

    #[test]
    fn flame_clears_3x3() {
        let mut b = base();
        red(&mut b, &[(4, 5), (4, 6)]);
        make_special(&mut b, (4, 4), Special::Flame);
        // 3x3 block (9 cells) plus the far end of the match at (4,6).
        assert_eq!(cleared_by(&b), 10);
    }

    #[test]
    fn star_clears_row_and_column() {
        let mut b = base();
        red(&mut b, &[(4, 5), (4, 6)]);
        make_special(&mut b, (4, 4), Special::Star);
        assert_eq!(cleared_by(&b), 15);
    }

    #[test]
    fn supernova_clears_three_rows_and_three_columns() {
        let mut b = base();
        red(&mut b, &[(4, 5), (4, 6)]);
        make_special(&mut b, (4, 4), Special::Supernova);
        assert_eq!(cleared_by(&b), 39);
    }

    #[test]
    fn supernova_in_a_corner_is_clipped() {
        let mut b = base();
        red(&mut b, &[(0, 1), (0, 2)]);
        make_special(&mut b, (0, 0), Special::Supernova);
        assert_eq!(cleared_by(&b), 28);
    }

    #[test]
    fn hypercube_swap_clears_that_color() {
        let mut b = base();
        b.set(
            (0, 0),
            Some(Gem {
                color: Color::Red,
                special: Special::Hypercube,
            }),
        );
        let oranges = b
            .cells
            .iter()
            .flatten()
            .filter(|c| matches!(c, Some(g) if g.special != Special::Hypercube && g.color == Color::Orange))
            .count();
        let mut rng = Rng::new(1);
        let events = try_swap(&mut b, &mut rng, &Rules::default(), (0, 0), (0, 1)).unwrap();
        let first_clear = events
            .iter()
            .find_map(|e| match e {
                Event::Clear { cells } => Some(cells.len()),
                _ => None,
            })
            .unwrap();
        assert_eq!(first_clear, oranges + 1);
    }

    #[test]
    fn two_hypercubes_clear_the_board() {
        let mut b = base();
        for p in [(0, 0), (0, 1)] {
            b.set(
                p,
                Some(Gem {
                    color: Color::Red,
                    special: Special::Hypercube,
                }),
            );
        }
        let mut rng = Rng::new(1);
        let events = try_swap(&mut b, &mut rng, &Rules::default(), (0, 0), (0, 1)).unwrap();
        let first_clear = events
            .iter()
            .find_map(|e| match e {
                Event::Clear { cells } => Some(cells.len()),
                _ => None,
            })
            .unwrap();
        assert_eq!(first_clear, W * H);
    }

    #[test]
    fn bad_swaps_are_rejected_and_leave_the_board_alone() {
        let mut b = base();
        let before = b.clone();
        let mut rng = Rng::new(1);
        let rules = Rules::default();
        assert_eq!(
            try_swap(&mut b, &mut rng, &rules, (0, 0), (2, 2)),
            Err(SwapError::NotAdjacent)
        );
        assert_eq!(
            try_swap(&mut b, &mut rng, &rules, (0, 0), (0, 8)),
            Err(SwapError::OutOfBounds)
        );
        assert_eq!(
            try_swap(&mut b, &mut rng, &rules, (0, 0), (0, 1)),
            Err(SwapError::NoMatch)
        );
        assert_eq!(b, before);
    }

    #[test]
    fn find_move_returns_a_swap_that_matches() {
        let mut b = base();
        // Red red _ red: swapping (0,2) with (0,3)... gives a legal move.
        red(&mut b, &[(0, 0), (0, 1), (0, 3)]);
        let (x, y) = find_move(&b, &Rules::default()).expect("a move exists");
        b.swap(x, y);
        assert!(!find_groups(&b, &Rules::default(), &[x, y]).is_empty());
    }

    #[test]
    fn new_boards_are_clean_playable_and_reproducible() {
        let rules = Rules::default();
        for seed in 0..30u64 {
            let b = Board::random(&mut Rng::new(seed), &rules);
            assert!(find_groups(&b, &rules, &[]).is_empty());
            assert!(has_move(&b, &rules));
            assert_eq!(b, Board::random(&mut Rng::new(seed), &rules));
        }
    }

    #[test]
    fn long_random_game_keeps_the_board_consistent() {
        let mut g = Game::new(7, Rules::default());
        for _ in 0..300 {
            if g.game_over {
                break;
            }
            let (a, b) = g.hint().expect("a move exists while the game is live");
            g.play(a, b).unwrap();
            assert!(g
                .board
                .cells
                .iter()
                .all(|row| row.iter().all(|c| c.is_some())));
            assert!(find_groups(&g.board, &g.rules, &[]).is_empty());
        }
        assert!(g.gems_cleared > 0);
    }
}
