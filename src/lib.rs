#![cfg_attr(not(target_arch = "wasm32"), forbid(unsafe_code))]
// pedantic/nursery — вкусовые группы, их выключаем осознанно.
// `clippy::all` (корректность, подозрительное, сложность, скорость) НЕ
// выключаем: иначе `cargo clippy -- -D warnings` в CI ничего не проверяет.
#![allow(clippy::pedantic, clippy::nursery)]
pub mod error;
pub mod frontend;
pub mod generate;
pub mod ir;
pub mod layout;
pub mod pipeline;
pub mod sheet;
pub mod style;
#[cfg(target_arch = "wasm32")]
pub mod wasm;
