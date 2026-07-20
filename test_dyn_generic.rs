// Minimal reproduction: can generic methods be called on dyn trait?
trait Foo {
    fn bar<T: std::fmt::Debug>(&self, x: &T) -> String {
        format!("{:?}", x)
    }
}

struct Baz;
impl Foo for Baz {}

fn main() {
    let x: &dyn Foo = &Baz;
    println!("{}", x.bar::<i32>(&42));
}
