// unsafe запрещён везде, кроме wasm-моста. `test` тоже разрешён:
// мост собирается при каждом `cargo test`, иначе ошибка в нём
// всплывает только в CI — а это ровно то, что случилось с `empty` в
// pack_json: локально всё зелёное, на CI красное.
#![cfg_attr(all(not(target_arch = "wasm32"), not(test)), forbid(unsafe_code))]
// pedantic/nursery — вкусовые группы, их выключаем осознанно.
// `clippy::all` (корректность, подозрительное, сложность, скорость) НЕ
// выключаем: иначе `cargo clippy -- -D warnings` в CI ничего не проверяет.
#![allow(clippy::pedantic, clippy::nursery)]
pub mod drawio;
pub mod frontend;
pub mod generate;
pub mod ir;
pub mod layout;
pub mod pipeline;
pub mod sheet;
pub mod style;

#[cfg(any(target_arch = "wasm32", test))]
pub mod wasm;
