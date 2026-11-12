//! The size measurement's minimal rquickjs host: one script (argv[1]) on a
//! new runtime and full context, `console.log` to stdout.

use rquickjs::function::Rest;
use rquickjs::{Context, Function, Object, Runtime};

fn main() {
    let src = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "console.log('hello')".to_owned());
    let Ok(rt) = Runtime::new() else {
        std::process::exit(1)
    };
    let Ok(ctx) = Context::full(&rt) else {
        std::process::exit(1)
    };
    let ok = ctx.with(|ctx| -> rquickjs::Result<()> {
        let console = Object::new(ctx.clone())?;
        console.set(
            "log",
            Function::new(ctx.clone(), |a: Rest<String>| println!("{}", a.0.join(" ")))?,
        )?;
        ctx.globals().set("console", console)?;
        ctx.eval::<(), _>(src)
    });
    if ok.is_err() {
        std::process::exit(1);
    }
}
