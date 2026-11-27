/// Convert LaTeX math expressions to clean, readable Unicode typography
/// (handles Greek letters, operators, arrows, sets, fractions, sqrt, superscripts & subscripts).
pub fn latex_to_unicode(input: &str) -> String {
    let mut s = input.trim().to_string();

    // Handle \frac{a}{b} -> (a)/(b) or a/b
    while let Some(pos) = s.find("\\frac") {
        let rest = &s[pos + 5..];
        if let Some((num, after_num)) = take_braced(rest) {
            if let Some((den, after_den)) = take_braced(after_num) {
                let n = latex_to_unicode(&num);
                let d = latex_to_unicode(&den);
                let repl = if n.chars().count() <= 3 && d.chars().count() <= 3 {
                    format!("{n}/{d}")
                } else {
                    format!("({n}) / ({d})")
                };
                s = format!("{}{}{}", &s[..pos], repl, after_den);
                continue;
            }
        }
        break;
    }

    // Handle \sqrt{x} -> √(x)
    while let Some(pos) = s.find("\\sqrt") {
        let rest = &s[pos + 5..];
        if let Some((arg, after)) = take_braced(rest) {
            let inner = latex_to_unicode(&arg);
            let repl = if inner.chars().count() == 1 {
                format!("√{inner}")
            } else {
                format!("√({inner})")
            };
            s = format!("{}{}{}", &s[..pos], repl, after);
            continue;
        }
        break;
    }

    // Strip formatting commands \text{...}, \mathrm{...}, \mathbf{...}, \mathcal{...}
    for cmd in ["\\text", "\\mathrm", "\\mathbf", "\\mathit", "\\mathcal", "\\mathbb", "\\operatorname"] {
        while let Some(pos) = s.find(cmd) {
            let rest = &s[pos + cmd.len()..];
            if let Some((arg, after)) = take_braced(rest) {
                s = format!("{}{}{}", &s[..pos], arg, after);
                continue;
            }
            break;
        }
    }

    // Replace known LaTeX symbols (longest first where prefixes overlap)
    const SYMBOLS: &[(&str, &str)] = &[
        ("\\Longleftrightarrow", "⟺"),
        ("\\Longrightarrow", "⟹"),
        ("\\Longleftarrow", "⟸"),
        ("\\leftrightarrow", "↔"),
        ("\\Leftrightarrow", "⇔"),
        ("\\rightarrow", "→"),
        ("\\Rightarrow", "⇒"),
        ("\\leftarrow", "←"),
        ("\\Leftarrow", "⇐"),
        ("\\mapsto", "↦"),
        ("\\subsetneq", "⊊"),
        ("\\subseteq", "⊆"),
        ("\\supseteq", "⊇"),
        ("\\subset", "⊂"),
        ("\\supset", "⊃"),
        ("\\approx", "≈"),
        ("\\equiv", "≡"),
        ("\\propto", "∝"),
        ("\\partial", "∂"),
        ("\\nabla", "∇"),
        ("\\infty", "∞"),
        ("\\forall", "∀"),
        ("\\exists", "∃"),
        ("\\notin", "∉"),
        ("\\times", "×"),
        ("\\cdot", "·"),
        ("\\cdots", "⋯"),
        ("\\ldots", "…"),
        ("\\dots", "…"),
        ("\\alpha", "α"),
        ("\\beta", "β"),
        ("\\gamma", "γ"),
        ("\\Gamma", "Γ"),
        ("\\delta", "δ"),
        ("\\Delta", "Δ"),
        ("\\varepsilon", "ε"),
        ("\\epsilon", "ϵ"),
        ("\\zeta", "ζ"),
        ("\\theta", "θ"),
        ("\\Theta", "Θ"),
        ("\\iota", "ι"),
        ("\\kappa", "κ"),
        ("\\lambda", "λ"),
        ("\\Lambda", "Λ"),
        ("\\sigma", "σ"),
        ("\\Sigma", "Σ"),
        ("\\upsilon", "υ"),
        ("\\varphi", "φ"),
        ("\\phi", "ϕ"),
        ("\\Phi", "Φ"),
        ("\\omega", "ω"),
        ("\\Omega", "Ω"),
        ("\\eta", "η"),
        ("\\mu", "μ"),
        ("\\nu", "ν"),
        ("\\xi", "ξ"),
        ("\\Xi", "Ξ"),
        ("\\pi", "π"),
        ("\\Pi", "Π"),
        ("\\rho", "ρ"),
        ("\\tau", "τ"),
        ("\\chi", "χ"),
        ("\\psi", "ψ"),
        ("\\Psi", "Ψ"),
        ("\\sum", "∑"),
        ("\\prod", "∏"),
        ("\\int", "∫"),
        ("\\oint", "∮"),
        ("\\pm", "±"),
        ("\\mp", "∓"),
        ("\\div", "÷"),
        ("\\leq", "≤"),
        ("\\le", "≤"),
        ("\\geq", "≥"),
        ("\\ge", "≥"),
        ("\\neq", "≠"),
        ("\\ne", "≠"),
        ("\\sim", "∼"),
        ("\\in", "∈"),
        ("\\cup", "∪"),
        ("\\cap", "∩"),
        ("\\emptyset", "∅"),
        ("\\varnothing", "∅"),
        ("\\land", "∧"),
        ("\\lor", "∨"),
        ("\\lnot", "¬"),
        ("\\neg", "¬"),
        ("\\to", "→"),
        ("\\gets", "←"),
        ("\\left", ""),
        ("\\right", ""),
        ("\\,", " "),
        ("\\;", " "),
        ("\\quad", "  "),
        ("\\qquad", "    "),
        ("\\\\", "\n"),
    ];

    for &(pat, rep) in SYMBOLS {
        s = s.replace(pat, rep);
    }

    // Superscripts & subscripts (e.g. ^2, ^{n+1}, _i, _{ij})
    s = convert_scripts(&s);

    // Clean leftover single braces
    s = s.replace(['{', '}'], "");
    s.trim().to_string()
}

fn take_braced(s: &str) -> Option<(String, &str)> {
    let trimmed = s.trim_start();
    if !trimmed.starts_with('{') {
        return None;
    }
    let mut depth = 0usize;
    for (i, ch) in trimmed.char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some((trimmed[1..i].to_string(), &trimmed[i + 1..]));
                }
            }
            _ => {}
        }
    }
    None
}

fn convert_scripts(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if (chars[i] == '^' || chars[i] == '_') && i + 1 < chars.len() {
            let is_sup = chars[i] == '^';
            if chars[i + 1] == '{' {
                if let Some(end) = (i + 2..chars.len()).find(|&j| chars[j] == '}') {
                    let group: String = chars[i + 2..end].iter().collect();
                    let converted: Option<String> = group
                        .chars()
                        .map(|c| if is_sup { to_sup(c) } else { to_sub(c) })
                        .collect();
                    if let Some(c) = converted {
                        out.push_str(&c);
                    } else {
                        out.push(chars[i]);
                        out.push('(');
                        out.push_str(&group);
                        out.push(')');
                    }
                    i = end + 1;
                    continue;
                }
            } else if let Some(mapped) = if is_sup {
                to_sup(chars[i + 1])
            } else {
                to_sub(chars[i + 1])
            } {
                out.push(mapped);
                i += 2;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

fn to_sup(c: char) -> Option<char> {
    Some(match c {
        '0' => '⁰', '1' => '¹', '2' => '²', '3' => '³', '4' => '⁴',
        '5' => '⁵', '6' => '⁶', '7' => '⁷', '8' => '⁸', '9' => '⁹',
        '+' => '⁺', '-' => '⁻', '=' => '⁼', '(' => '⁽', ')' => '⁾',
        'n' => 'ⁿ', 'i' => 'ⁱ', 'x' => 'ˣ', 'y' => 'ʸ', 'k' => 'ᵏ', 't' => 'ᵗ',
        _ => return None,
    })
}

fn to_sub(c: char) -> Option<char> {
    Some(match c {
        '0' => '₀', '1' => '₁', '2' => '₂', '3' => '₃', '4' => '₄',
        '5' => '₅', '6' => '₆', '7' => '₇', '8' => '₈', '9' => '₉',
        '+' => '₊', '-' => '₋', '=' => '₌', '(' => '₍', ')' => '₎',
        'a' => 'ₐ', 'e' => 'ₑ', 'i' => 'ᵢ', 'j' => 'ⱼ', 'k' => 'ₖ',
        'm' => 'ₘ', 'n' => 'ₙ', 'o' => 'ₒ', 'p' => 'ₚ', 'r' => 'ᵣ',
        's' => 'ₛ', 't' => 'ₜ', 'u' => 'ᵤ', 'v' => 'ᵥ', 'x' => 'ₓ',
        _ => return None,
    })
}
