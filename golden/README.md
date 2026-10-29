# golden

Эталонные SVG для `tests/golden.rs`: раскладка C++-схемы не должна
меняться незаметно.

`cpp-lab.svg` — `main` из `corpus/cpp-lab.cpp`, снятый CLI. Пересобрать
после осознанного изменения раскладки:

```bash
cargo run --release -- corpus/cpp-lab.cpp -o /tmp/gostpadi-golden
cp /tmp/gostpadi-golden/cpp-lab.svg golden/cpp-lab.svg
cargo test --test golden
```
