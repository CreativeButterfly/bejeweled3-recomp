use bej3core::{Event, Game, Rules};
use std::io::{self, BufRead, Write};

fn describe(e: &Event) -> String {
    match e {
        Event::Swap { a, b } => format!("swap {a:?} and {b:?}"),
        Event::Clear { cells } => format!("clear {} gems", cells.len()),
        Event::Spawn { pos, gem } => format!("create {:?} {:?} at {pos:?}", gem.color, gem.special),
        Event::Trigger { pos, special } => format!("{special:?} fires at {pos:?}"),
        Event::Fall { moves } => format!("{} gems fall", moves.len()),
        Event::Fill { gems } => format!("{} new gems drop in", gems.len()),
        Event::NoMoves => "no legal moves left".to_string(),
    }
}

fn main() {
    let seed: u64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    let mut game = Game::new(seed, Rules::default());

    println!("terminal harness, seed {seed}");
    println!("type: row col row col   (0-based) to swap two neighbors");
    println!("      h  for a hint,  q  to quit");
    println!("markers: * flame  + star  # supernova  @@ hypercube");

    let stdin = io::stdin();
    loop {
        println!();
        println!("{}", game.board.render());
        println!("moves {}   gems cleared {}", game.moves, game.gems_cleared);
        if game.game_over {
            println!("no moves left: game over");
            break;
        }
        print!("> ");
        io::stdout().flush().ok();

        let mut line = String::new();
        if stdin.lock().read_line(&mut line).unwrap_or(0) == 0 {
            break;
        }
        let text = line.trim();
        if text == "q" {
            break;
        }
        if text == "h" {
            match game.hint() {
                Some(((r1, c1), (r2, c2))) => println!("try: {r1} {c1} {r2} {c2}"),
                None => println!("no hint available"),
            }
            continue;
        }

        let nums: Vec<usize> = text
            .split_whitespace()
            .filter_map(|x| x.parse().ok())
            .collect();
        if nums.len() != 4 {
            println!("need four numbers: row col row col");
            continue;
        }
        match game.play((nums[0], nums[1]), (nums[2], nums[3])) {
            Ok(events) => {
                for e in &events {
                    println!("  {}", describe(e));
                }
            }
            Err(err) => println!("{err}"),
        }
    }
}
