// Test: async fn + generic, dyn compatible?
use std::future::Future;

trait Foo {
    async fn bar<T: std::fmt::Debug>(&self, x: &T) -> String {
        format!("{:?}", x)
    }
}

struct Baz;
impl Foo for Baz {}

fn main() {
    let x: &dyn Foo = &Baz;
    // actually call through the dyn:
    let fut: std::pin::Pin<Box<dyn Future<Output = String> + '_>> = Box::pin(x.bar::<i32>(&42));
    println!("would block here");
}
