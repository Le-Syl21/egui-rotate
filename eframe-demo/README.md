# egui-rotate — eframe demo (native + web)

An [eframe](https://github.com/emilk/egui/tree/master/crates/eframe) application
showing [`egui-rotate`](..)'s `RotationPlugin`:

- the **main window** rotates (button `↻` / key `R`),
- a **child window** (button *Open child window*) is a *separate OS window* with
  its **own** rotation — same plugin, same `Context`, different viewport id,
- an animated **stress test** (the *animated shapes* slider) shows the per-frame
  rotation cost stays negligible — watch the FPS readout.

The same code runs natively and in the browser, which is the point: registering
`RotationPlugin` is the entire integration on both.

## Run native

```bash
cargo run --release
```

## Run on the web

Needs the wasm target and [Trunk](https://trunkrs.dev):

```bash
rustup target add wasm32-unknown-unknown
cargo install --locked trunk
trunk serve --release      # then open http://127.0.0.1:8080
```

> Note: child windows (`show_viewport_immediate`) become **embedded** windows on
> the web (the browser has no native multi-window), so on the web the child shows
> inside the page. Native gives a real second OS window.

> The wasm target compiles (`cargo build --target wasm32-unknown-unknown --lib`),
> but the browser run via `trunk serve` was not exercised where this was written.
> The app logic in `src/app.rs` is shared with the native build, which is tested.
