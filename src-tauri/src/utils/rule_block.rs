const RULES_START: &str = "<!-- AgentHarbor:rules:start -->";
const RULES_END: &str = "<!-- AgentHarbor:rules:end -->";

fn rule_marker(rule_id: &str) -> String {
    format!("<!-- AgentHarbor:rule:{} -->", rule_id)
}

/// Strip the first heading line (e.g. "## Rule Name") from rule body so it
/// isn't duplicated below the numbered bold title we inject.
fn strip_leading_heading(body: &str) -> &str {
    let s = body.trim_start();
    if s.starts_with('#') {
        if let Some(nl) = s.find('\n') {
            s[nl..].trim_start_matches('\n')
        } else {
            ""
        }
    } else {
        s
    }
}

fn count_rules_in_block(block: &str) -> usize {
    block.matches("<!-- AgentHarbor:rule:").count()
}

/// Renumber "N. **Title**" lines that immediately follow each rule marker.
fn renumber_rules(block: &str) -> String {
    let mut num = 1usize;
    let mut after_marker = false;
    let mut lines_out: Vec<String> = Vec::new();

    for line in block.lines() {
        if line.starts_with("<!-- AgentHarbor:rule:") {
            after_marker = true;
            lines_out.push(line.to_string());
        } else if after_marker && !line.trim().is_empty() {
            if let Some(dot_pos) = line.find(". **") {
                let before = &line[..dot_pos];
                if !before.is_empty() && before.chars().all(|c| c.is_ascii_digit()) {
                    lines_out.push(format!("{}{}", num, &line[dot_pos..]));
                    num += 1;
                    after_marker = false;
                    continue;
                }
            }
            after_marker = false;
            lines_out.push(line.to_string());
        } else {
            lines_out.push(line.to_string());
        }
    }

    let joined = lines_out.join("\n");
    if block.ends_with('\n') && !joined.ends_with('\n') {
        format!("{}\n", joined)
    } else {
        joined
    }
}

pub fn has_rule(content: &str, rule_id: &str) -> bool {
    content.contains(&rule_marker(rule_id))
}

/// Inject a rule into content.
/// - If the rule ID already exists: update name + body in place, keep number.
/// - If the managed block exists: append with next sequential number.
/// - If no block: create `## Rules` section at end of file.
pub fn inject_rule(content: &str, rule_id: &str, rule_name: &str, rule_body: &str) -> String {
    let marker = rule_marker(rule_id);
    let body = strip_leading_heading(rule_body).trim_end();

    // Case 1: rule already exists — update in place
    if let Some(m_start) = content.find(&marker) {
        let after = &content[m_start + marker.len()..];
        let rule_end_offset = after
            .find("<!-- AgentHarbor:rule:")
            .or_else(|| after.find(RULES_END))
            .unwrap_or(after.len());
        let rule_end = m_start + marker.len() + rule_end_offset;

        let existing = &content[m_start + marker.len()..rule_end];
        let num = existing
            .lines()
            .find(|l| !l.trim().is_empty())
            .and_then(|l| {
                let dot = l.find(". **")?;
                l[..dot].parse::<usize>().ok()
            })
            .unwrap_or(1);

        let new_chunk = format!("\n{}. **{}**\n\n{}\n\n", num, rule_name, body);
        return format!(
            "{}{}{}{}",
            &content[..m_start],
            marker,
            new_chunk,
            &content[rule_end..]
        );
    }

    // Case 2: block exists — append numbered entry before the end marker
    if let (Some(start_pos), Some(end_pos)) = (content.find(RULES_START), content.find(RULES_END)) {
        let block_content = &content[start_pos + RULES_START.len()..end_pos];
        let next_num = count_rules_in_block(block_content) + 1;
        let new_entry = format!(
            "\n{}\n{}. **{}**\n\n{}\n",
            marker, next_num, rule_name, body
        );
        return format!("{}{}{}", &content[..end_pos], new_entry, &content[end_pos..]);
    }

    // Case 3: no block — create section at end of file
    let block = format!(
        "\n## Rules\n\nFollow all rules in this section. Deployed via AgentHarbor.\n\n{}\n{}\n1. **{}**\n\n{}\n{}\n",
        RULES_START, marker, rule_name, body, RULES_END
    );
    let mut result = content.to_string();
    if !result.is_empty() && !result.ends_with('\n') {
        result.push('\n');
    }
    result.push_str(&block);
    result
}

/// Remove a rule by ID and renumber remaining rules sequentially.
pub fn remove_rule(content: &str, rule_id: &str) -> String {
    let marker = rule_marker(rule_id);
    let Some(m_start) = content.find(&marker) else {
        return content.to_string();
    };

    let after = &content[m_start + marker.len()..];
    let rule_end_offset = after
        .find("<!-- AgentHarbor:rule:")
        .or_else(|| after.find(RULES_END))
        .unwrap_or(after.len());
    let rule_end = m_start + marker.len() + rule_end_offset;

    // Eat the leading newline before the marker so we don't leave a blank line
    let chunk_start = if m_start > 0 && content.as_bytes().get(m_start - 1) == Some(&b'\n') {
        m_start - 1
    } else {
        m_start
    };

    let removed = format!("{}{}", &content[..chunk_start], &content[rule_end..]);

    // Renumber remaining rules inside the block
    if let (Some(s), Some(e)) = (removed.find(RULES_START), removed.find(RULES_END)) {
        let before = &removed[..s + RULES_START.len()];
        let block = &removed[s + RULES_START.len()..e];
        let after_block = &removed[e..];
        format!("{}{}{}", before, renumber_rules(block), after_block)
    } else {
        removed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inject_first_rule_creates_block() {
        let content = "# My Project\n\nSome context.\n";
        let result = inject_rule(content, "abc123", "No Magic Numbers", "Extract literals into constants.");
        assert!(result.contains("## Rules"));
        assert!(result.contains(RULES_START));
        assert!(result.contains("<!-- AgentHarbor:rule:abc123 -->"));
        assert!(result.contains("1. **No Magic Numbers**"));
        assert!(result.contains(RULES_END));
    }

    #[test]
    fn test_inject_strips_leading_heading() {
        let content = "";
        let body = "## No Magic Numbers\n\nExtract literals.";
        let result = inject_rule(content, "r1", "No Magic Numbers", body);
        // Should not have the heading duplicated after the bold title
        let count = result.matches("No Magic Numbers").count();
        assert_eq!(count, 1, "Heading should appear only once, got:\n{}", result);
    }

    #[test]
    fn test_inject_second_rule_increments_number() {
        let existing = format!(
            "# Project\n\n## Rules\n\nFollow all rules.\n\n{}\n<!-- AgentHarbor:rule:r1 -->\n1. **First Rule**\n\nDo this.\n{}\n",
            RULES_START, RULES_END
        );
        let result = inject_rule(&existing, "r2", "Second Rule", "Do that.");
        assert!(result.contains("2. **Second Rule**"));
        assert!(result.contains("<!-- AgentHarbor:rule:r2 -->"));
    }

    #[test]
    fn test_update_existing_rule_keeps_number() {
        let existing = format!(
            "## Rules\n\n{}\n<!-- AgentHarbor:rule:r1 -->\n1. **Old Name**\n\nOld content.\n{}\n",
            RULES_START, RULES_END
        );
        let result = inject_rule(&existing, "r1", "New Name", "New content.");
        assert!(result.contains("1. **New Name**"));
        assert!(result.contains("New content."));
        assert!(!result.contains("Old Name"));
        assert_eq!(result.matches("<!-- AgentHarbor:rule:r1 -->").count(), 1);
    }

    #[test]
    fn test_remove_and_renumber() {
        let existing = format!(
            "{}\n<!-- AgentHarbor:rule:r1 -->\n1. **Rule One**\n\nC1.\n\n<!-- AgentHarbor:rule:r2 -->\n2. **Rule Two**\n\nC2.\n{}",
            RULES_START, RULES_END
        );
        let result = remove_rule(&existing, "r1");
        assert!(!result.contains("Rule One"));
        assert!(result.contains("1. **Rule Two**"), "Expected renumbered to 1, got:\n{}", result);
        assert!(!result.contains("2. **Rule Two**"));
    }

    #[test]
    fn test_has_rule() {
        let content = "<!-- AgentHarbor:rule:abc123 -->\n1. **Test**\n";
        assert!(has_rule(content, "abc123"));
        assert!(!has_rule(content, "other"));
    }
}
