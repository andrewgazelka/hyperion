// #![feature(lint_reasons)]
// #![allow(clippy::missing_docs_in_private_items, missing_docs, reason = "bench")]
//
// use std::{
//     net::SocketAddr,
//     sync::{
//         atomic::{AtomicU16, AtomicU32},
//         Arc,
//     },
// };
//
// use divan::{AllocProfiler, Bencher};
// use libc::{getrlimit, setrlimit, RLIMIT_NOFILE};
// // use rust_mc_bot::{Address, BotManager};
// use server::Game;
//
// static PORT: AtomicU16 = AtomicU16::new(25565);
//
// #[global_allocator]
// static ALLOC: AllocProfiler = AllocProfiler::system();
//
// fn adjust_file_limits() {
//     unsafe {
//         let mut limits = libc::rlimit {
//             rlim_cur: 0, // Initialize soft limit to 0
//             rlim_max: 0, // Initialize hard limit to 0
//         };
//
//         if getrlimit(RLIMIT_NOFILE, &mut limits) == 0 {
//             println!("Current soft limit: {}", limits.rlim_cur);
//             println!("Current hard limit: {}", limits.rlim_max);
//         } else {
//             eprintln!("Failed to get the maximum number of open file descriptors");
//         }
//
//         limits.rlim_cur = limits.rlim_max;
//         println!("Setting soft limit to: {}", limits.rlim_cur);
//
//         if setrlimit(RLIMIT_NOFILE, &limits) != 0 {
//             eprintln!("Failed to set the maximum number of open file descriptors");
//         }
//     }
// }
//
// fn main() {
//     // this is to make sure we don't run out of file descriptors
//     adjust_file_limits();
//
//     divan::main();
// }
//
// const PLAYER_COUNTS: &[u32] = &[1, 2, 4, 8, 16, 32, 64, 128, 256];
//
// #[divan::bench(
//     args = PLAYER_COUNTS,
// )]
// fn n_bots_moving(bencher: Bencher, player_count: u32) {
//     let port = PORT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
//     let addr = SocketAddr::from(([127, 0, 0, 1], port));
//
//     let mut game = Game::init(addr).unwrap();
//
//     let addrs = Address::TCP(addr);
//     let mut bot_manager =
//         BotManager::create(player_count, addrs, 0, Arc::new(AtomicU32::new(0))).unwrap();
//
//     loop {
//         game.tick();
//         bot_manager.tick();
//
//         if game
//             .shared()
//             .player_count
//             .load(std::sync::atomic::Ordering::Relaxed)
//             == player_count
//         {
//             break;
//         }
//     }
//
//     // we have completed the login sequence for all bots, now we can start benchmarking
//
//     bencher.bench_local(|| {
//         game.tick();
//         bot_manager.tick();
//     });
//
//     game.shutdown();
// }
