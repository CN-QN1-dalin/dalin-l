//! Dalin L 标准库 — 内置函数实现层
//!
//! 这些函数在 Dalin L 中可以作为内置函数直接调用，
//! 底层由 Rust 实现，提供高性能的基本操作。

/// 列表操作
pub mod list {
    pub fn map<A, B>(items: Vec<A>, f: impl Fn(A) -> B) -> Vec<B> {
        items.into_iter().map(f).collect()
    }

    pub fn filter<A>(items: Vec<A>, pred: impl Fn(&A) -> bool) -> Vec<A> {
        items.into_iter().filter(pred).collect()
    }

    pub fn reduce<A>(items: Vec<A>, f: impl Fn(A, A) -> A) -> Option<A> {
        items.into_iter().reduce(f)
    }

    pub fn len<T>(items: &[T]) -> usize {
        items.len()
    }

    pub fn push<T>(items: &mut Vec<T>, val: T) {
        items.push(val);
    }
}

/// 字符串操作
pub mod string {
    pub fn split(s: &str, delim: &str) -> Vec<String> {
        s.split(delim).map(|p| p.to_string()).collect()
    }

    pub fn trim(s: &str) -> String {
        s.trim().to_string()
    }

    pub fn to_upper(s: &str) -> String {
        s.to_uppercase()
    }

    pub fn to_lower(s: &str) -> String {
        s.to_lowercase()
    }

    pub fn len(s: &str) -> usize {
        s.len()
    }

    pub fn contains(s: &str, sub: &str) -> bool {
        s.contains(sub)
    }
}

/// IO 操作
pub mod io {
    pub fn print(s: &str) {
        print!("{s}");
    }

    pub fn println(s: &str) {
        println!("{s}");
    }

    pub fn read_line() -> String {
        let mut buf = String::new();
        std::io::stdin().read_line(&mut buf).ok();
        buf.trim().to_string()
    }
}

/// 数学操作
pub mod math {
    pub fn abs(x: i64) -> i64 {
        x.abs()
    }

    pub fn max(a: i64, b: i64) -> i64 {
        a.max(b)
    }

    pub fn min(a: i64, b: i64) -> i64 {
        a.min(b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_map() {
        let r = list::map(vec![1, 2, 3], |x| x * 2);
        assert_eq!(r, vec![2, 4, 6]);
    }

    #[test]
    fn test_list_filter() {
        let r = list::filter(vec![1, 2, 3, 4], |x| x % 2 == 0);
        assert_eq!(r, vec![2, 4]);
    }

    #[test]
    fn test_list_reduce() {
        let r = list::reduce(vec![1, 2, 3, 4], |a, b| a + b);
        assert_eq!(r, Some(10));
    }

    #[test]
    fn test_string_split() {
        let r = string::split("a,b,c", ",");
        assert_eq!(r, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_string_trim() {
        assert_eq!(string::trim("  hello  "), "hello");
    }

    #[test]
    fn test_string_upper() {
        assert_eq!(string::to_upper("hello"), "HELLO");
    }

    #[test]
    fn test_math_ops() {
        assert_eq!(math::abs(-5), 5);
        assert_eq!(math::max(3, 7), 7);
        assert_eq!(math::min(3, 7), 3);
    }
}
