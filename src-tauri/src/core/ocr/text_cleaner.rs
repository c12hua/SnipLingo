/// 判断字符是否为中日韩 (CJK) 文字（汉字、平假名、片假名、韩文音节等）
pub fn is_cjk_char(c: char) -> bool {
    matches!(c,
        '\u{4E00}'..='\u{9FFF}' |       // CJK Unified Ideographs
        '\u{3400}'..='\u{4DBF}' |       // CJK Extension A
        '\u{20000}'..='\u{2A6DF}' |     // CJK Extension B
        '\u{2A700}'..='\u{2B73F}' |     // CJK Extension C
        '\u{2B740}'..='\u{2B81F}' |     // CJK Extension D
        '\u{2B820}'..='\u{2CEAF}' |     // CJK Extension E
        '\u{2CEB0}'..='\u{2EBEF}' |     // CJK Extension F
        '\u{30000}'..='\u{3134F}' |     // CJK Extension G
        '\u{F900}'..='\u{FAFF}' |       // CJK Compatibility Ideographs
        '\u{3040}'..='\u{309F}' |       // Hiragana
        '\u{30A0}'..='\u{30FF}' |       // Katakana
        '\u{31F0}'..='\u{31FF}' |       // Katakana Phonetic Extensions
        '\u{AC00}'..='\u{D7AF}' |       // Hangul Syllables
        '\u{1100}'..='\u{11FF}' |       // Hangul Jamo
        '\u{3130}'..='\u{318F}'         // Hangul Compatibility Jamo
    )
}

/// 判断字符是否为 CJK 左侧/前置标点符号（如开引号、开括号、开书名号等）
pub fn is_cjk_opening_punct(c: char) -> bool {
    matches!(c,
        '“' | '‘' | '（' | '《' | '〈' | '「' | '『' | '【' | '〔' | '〖' | '〘' | '〚' |
        '\u{FE41}' | '\u{FE43}' // ﹁ ﹂
    )
}

/// 判断字符是否为 CJK 右侧/后置或句末标点符号（如闭引号、闭括号、逗号、句号、冒号、分号等）
pub fn is_cjk_closing_or_terminal_punct(c: char) -> bool {
    matches!(c,
        '”' | '’' | '）' | '》' | '〉' | '」' | '』' | '】' | '〕' | '〗' | '〙' | '〛' |
        '，' | '。' | '、' | '；' | '：' | '！' | '？' | '．' |
        '\u{2014}' | '\u{2015}' | '\u{2026}' | '\u{00B7}'
    )
}

/// 是否为任意 CJK 标点
pub fn is_cjk_punct(c: char) -> bool {
    is_cjk_opening_punct(c) || is_cjk_closing_or_terminal_punct(c)
}

/// 针对单行内消除 Windows OCR 等引擎在汉字间强行插入的无意义空格
pub fn clean_cjk_spaces_in_line(line: &str) -> String {
    let chars: Vec<char> = line.chars().collect();
    if chars.is_empty() {
        return String::new();
    }

    let mut result = String::with_capacity(line.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == ' ' || c == '\t' || c == '\u{3000}' {
            // 查看空格前后的非空格字符
            let prev_char = result.chars().last();

            // 统计并跳过连续空格
            let space_start = i;
            while i < chars.len() && (chars[i] == ' ' || chars[i] == '\t' || chars[i] == '\u{3000}') {
                i += 1;
            }
            let next_char = if i < chars.len() { Some(chars[i]) } else { None };

            match (prev_char, next_char) {
                (None, _) => {
                    // 行首缩进/空格：保持原有前置空格
                    for _ in space_start..i {
                        result.push(c);
                    }
                }
                (_, None) => {
                    // 行尾空格：忽略
                }
                (Some(p), Some(n)) => {
                    let should_remove =
                        // 1. 汉字 与 汉字 之间 
                        (is_cjk_char(p) && is_cjk_char(n)) ||
                        // 2. 汉字 与 CJK 标点之间 
                        (is_cjk_char(p) && is_cjk_punct(n)) ||
                        // 3. CJK 标点 与 汉字之间
                        (is_cjk_punct(p) && is_cjk_char(n)) ||
                        // 4. CJK 标点 与 CJK 标点之间
                        (is_cjk_punct(p) && is_cjk_punct(n)) ||
                        // 5. CJK 开标点与后方任意字符之间
                        is_cjk_opening_punct(p) ||
                        // 6. 任意字符与 CJK 闭标点之间
                        is_cjk_closing_or_terminal_punct(n);

                    if !should_remove {
                        // 保留单个空格（如英文与英文之间，或中文与英文单词之间）
                        result.push(' ');
                    }
                }
            }
        } else {
            result.push(c);
            i += 1;
        }
    }

    result
}

/// 自然段落行合并：智能处理行末连字符与中西文行连接空格
fn merge_paragraph_lines(lines: &[&str]) -> String {
    let mut para = String::new();
    for line in lines {
        let cleaned_line = clean_cjk_spaces_in_line(line.trim());
        if cleaned_line.is_empty() {
            continue;
        }
        if para.is_empty() {
            para.push_str(&cleaned_line);
        } else {
            let last_char = para.chars().last();
            let first_char = cleaned_line.chars().next();
            match (last_char, first_char) {
                (Some(p), Some(n)) => {
                    let need_space = !(
                        (is_cjk_char(p) && is_cjk_char(n)) ||
                        (is_cjk_char(p) && is_cjk_punct(n)) ||
                        (is_cjk_punct(p) && is_cjk_char(n)) ||
                        (is_cjk_punct(p) && is_cjk_punct(n)) ||
                        is_cjk_opening_punct(p) ||
                        is_cjk_closing_or_terminal_punct(n)
                    );
                    if need_space {
                        para.push(' ');
                    }
                    para.push_str(&cleaned_line);
                }
                _ => {
                    para.push_str(&cleaned_line);
                }
            }
        }
    }
    para
}

/// 针对 OCR 识别出的文本进行智能清洗与排版归一化
/// preserve_line_breaks: 若为 true，保持代码与表格换行；若为 false，将单行换行合并为连贯段落
pub fn clean_ocr_text_with_options(raw: &str, preserve_line_breaks: bool) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    if preserve_line_breaks {
        // 保持换行模式：适合代码、报错堆栈日志、表格与终端文本
        let normalized = trimmed.replace("\r\n", "\n");
        let lines: Vec<String> = normalized
            .lines()
            .map(|l| clean_cjk_spaces_in_line(l.trim_end()))
            .collect();
        return lines.join("\n");
    }

    // 自然段落合并模式：
    // 1. 处理英文连字符换行 (如 "differ-\nent" -> "different")
    let unhyphenated = trimmed.replace("-\r\n", "").replace("-\n", "");

    // 2. 将段落内的单个换行智能连接（中文字符无缝衔接，英文单词保留空格），保留双换行（分段）
    let normalized = unhyphenated.replace("\r\n", "\n");
    let raw_paragraphs: Vec<&str> = normalized.split("\n\n").collect();

    let cleaned_paragraphs: Vec<String> = raw_paragraphs
        .into_iter()
        .map(|para| {
            let lines: Vec<&str> = para.lines().collect();
            merge_paragraph_lines(&lines)
        })
        .filter(|p| !p.is_empty())
        .collect();

    cleaned_paragraphs.join("\n\n")
}

/// 默认兼容接口 (合并段落)
pub fn clean_ocr_text(raw: &str) -> String {
    clean_ocr_text_with_options(raw, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_ocr_text_empty() {
        assert_eq!(clean_ocr_text("   \n\t  "), "");
    }

    #[test]
    fn test_clean_ocr_text_hyphen() {
        let input = "This is a sophisti-\ncated test.";
        assert_eq!(clean_ocr_text(input), "This is a sophisticated test.");
    }

    #[test]
    fn test_clean_ocr_text_paragraphs() {
        let input = "First line of para 1.\nSecond line of para 1.\n\nFirst line of para 2.";
        let expected = "First line of para 1. Second line of para 1.\n\nFirst line of para 2.";
        assert_eq!(clean_ocr_text(input), expected);
    }

    #[test]
    fn test_clean_ocr_text_preserve_line_breaks() {
        let code = "fn main() {\n    let a = 1;\n    println!(\"{}\", a);\n}";
        let res = clean_ocr_text_with_options(code, true);
        assert_eq!(res, code);
    }

    #[test]
    fn test_clean_cjk_spaces() {
        let input = "测 试 文本 处 理 ， 标 点 符 号 。";
        let expected = "测试文本处理，标点符号。";
        let res = clean_ocr_text(input);
        assert_eq!(res, expected);
    }
}
