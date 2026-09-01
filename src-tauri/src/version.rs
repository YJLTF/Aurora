use std::cmp::Ordering;

/// 比较两个版本号：返回 a 相对 b 的顺序。
/// 宽松解析：忽略前导 v/V 与标点，数字段按数值、字母段按字典序比较，
/// 缺失的数字段按 0 处理（因此 1.2 与 1.2.0 相等），可兼容 wezterm 这类日期版本号。
pub fn compare(a: &str, b: &str) -> Ordering {
    let sa = tokenize(normalize(a));
    let sb = tokenize(normalize(b));
    let n = sa.len().max(sb.len());
    for i in 0..n {
        let ord = match (sa.get(i), sb.get(i)) {
            (None, None) => Ordering::Equal,
            (None, Some(Seg::Num(v))) => 0u64.cmp(v),
            // 预发布约定：多出来的字母段（如 -beta）视为更旧
            (None, Some(Seg::Str(_))) => Ordering::Greater,
            (Some(Seg::Num(v)), None) => v.cmp(&0),
            (Some(Seg::Str(_)), None) => Ordering::Less,
            (Some(x), Some(y)) => cmp_seg(x, y),
        };
        if ord != Ordering::Equal {
            return ord;
        }
    }
    Ordering::Equal
}

fn normalize(v: &str) -> &str {
    let v = v.trim();
    v.strip_prefix(['v', 'V']).unwrap_or(v)
}

#[derive(Debug, PartialEq, Eq)]
enum Seg {
    Num(u64),
    Str(String),
}

fn cmp_seg(a: &Seg, b: &Seg) -> Ordering {
    match (a, b) {
        (Seg::Num(x), Seg::Num(y)) => x.cmp(y),
        (Seg::Str(x), Seg::Str(y)) => x.cmp(y),
        // 约定数字段比字母段“新”，符合 tag 后缀（如 -beta）的一般直觉
        (Seg::Num(_), Seg::Str(_)) => Ordering::Greater,
        (Seg::Str(_), Seg::Num(_)) => Ordering::Less,
    }
}

fn tokenize(s: &str) -> Vec<Seg> {
    let mut segs: Vec<Seg> = Vec::new();
    let mut cur = String::new();
    let mut cur_kind = '\0';
    for c in s.chars() {
        let k = if c.is_ascii_digit() {
            'd'
        } else if c.is_ascii_alphabetic() {
            'a'
        } else {
            '\0'
        };
        if !cur.is_empty() && k != cur_kind {
            push(&mut segs, cur_kind, &cur);
            cur.clear();
        }
        if k != '\0' {
            cur_kind = k;
            cur.push(c);
        }
    }
    push(&mut segs, cur_kind, &cur);
    segs
}

fn push(segs: &mut Vec<Seg>, kind: char, cur: &str) {
    if cur.is_empty() {
        return;
    }
    if kind == 'd' {
        segs.push(Seg::Num(cur.parse::<u64>().unwrap_or(u64::MAX)));
    } else if kind == 'a' {
        segs.push(Seg::Str(cur.to_lowercase()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Ordering::*;

    #[test]
    fn basic_semver() {
        assert_eq!(compare("1.2.10", "1.2.9"), Greater);
        assert_eq!(compare("3.8.2", "3.8.2"), Equal);
        assert_eq!(compare("v1.103.0", "1.102.3"), Greater);
        assert_eq!(compare("1.2", "1.2.0"), Equal);
        assert_eq!(compare("26.0.3", "9.9.9"), Greater);
    }

    #[test]
    fn date_tags() {
        assert_eq!(compare("20240203-110809-5046fc22", "20250601-073000-e693f822"), Less);
        assert_eq!(compare("20250601-073000-e693f822", "20250601-073000-e693f822"), Equal);
    }

    #[test]
    fn misc() {
        assert_eq!(compare("", ""), Equal);
        assert_eq!(compare("V2.1.0", "2.0.9"), Greater);
        assert_eq!(compare("2.1.0-beta", "2.1.0"), Less);
    }
}
