#[test]
fn f() {
    use goblin::Object;
    use std::fs;

    let buffer = fs::read("/home/hack3rmann/Downloads/libclntsh.so.12.1.0").unwrap();

    let Object::Elf(elf) = Object::parse(&buffer).unwrap() else {
        panic!()
    };

    dbg!(&elf);
}
