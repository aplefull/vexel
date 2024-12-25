#[macro_export]
macro_rules! time_fn {
    ($func:expr) => {{
        let start = Instant::now();
        let result = $func;
        let duration = start.elapsed();
        println!("Function '{}' executed in {:.2?}", stringify!($func), duration);
        result
    }};
}

#[macro_export]
macro_rules! time_block {
    ($block_name:expr, $block:block) => {{
        let start = Instant::now();
        let result = $block;
        let duration = start.elapsed();
        println!("Block '{}' executed in {:.2?}", $block_name, duration);
        result
    }};
}
