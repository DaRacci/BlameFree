# Jaccard Judge Cross-Validation: Rust vs Python

**Date:** 2026-06-29  
**Project:** review-harness (Rust `crb-judge`) vs code-review-benchmark-research (Python `step3_judge_comments.py`)

---

## 1. Summary

| Metric | Value |
|--------|-------|
| Rust tests passing | 10/10 cross-validation + 11 original = **21/21** |
| Python vs Rust identical results | **9/12 test cases (75%)** |
| Different results | 3/12 test cases (25%) |
| **Root cause** | Tokenization algorithm mismatch |

## 2. Key Finding: Tokenization Mismatch

The two implementations use **different tokenization strategies**, which produces different Jaccard scores for text containing punctuation.

### Python (step3_judge_comments.py, lines 42-43)

```python
gc_words = set(gc_text.split())    # whitespace-only split
cand_words = set(cand_text.split())
```

### Rust (crb-judge/src/lib.rs, lines 89-95)

```rust
fn tokenize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric() && c != '\'')  // non-alphanumeric split
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}
```

### Impact

| Input Text | Python Tokens | Rust Tokens |
|------------|---------------|-------------|
| `"cross-site scripting"` | `{"cross-site", "scripting"}` | `{"cross", "site", "scripting"}` |
| `"xss (cross-site scripting)"` | `{"xss", "(cross-site", "scripting)"}` | `{"xss", "cross", "site", "scripting"}` |
| `"open(url)"` | `{"open(url)"}` | `{"open", "url"}` |
| `"Server-Side Request Forgery"` | `{"server-side", "request", "forgery"}` | `{"server", "side", "request", "forgery"}` |
| `"doesn't work"` | `{"doesn't", "work"}` | `{"doesn't", "work"}` (apostrophe preserved) |

### Effects on Jaccard Scores

1. **Rust is more lenient** — hyphenated and parenthesized terms are split into component words, increasing the chance of overlap.
2. **Python is more strict** — punctuation-attached tokens like `"cross-site"` or `"open(url)"` are treated as atomic, so they only match identical token forms.
3. **Apostrophes are handled identically** — both preserve `"doesn't"` as a single token.
4. **Simple whitespace-separated text produces identical scores** — for the majority of real code review comments (which tend to be plain prose), the results are identical.

---

## 3. Side-by-Side Score Comparison

| # | Test Case | Python Score | Rust Score | Python Match | Rust Match | Verdict |
|---|-----------|-------------|-----------|-------------|-----------|---------|
| 1 | identical strings | 1.0000 | 1.0000 | ✅ | ✅ | **Identical** |
| 2 | partial overlap (hardcoded) | 0.1429 | 0.1429 | ✅ | ✅ | **Identical** |
| 3 | no overlap | 0.0000 | None (0.0) | ❌ | ❌ | **Identical** |
| 4 | **punctuation diff** `"xss (cross-site scripting)"` vs `"xss cross site scripting"` | **0.1667** | **1.0000** | ✅ | ✅ | **DIFFERENT score** |
| 5 | case insensitive | 1.0000 | 1.0000 | ✅ | ✅ | **Identical** |
| 6 | both empty | 0.0000 | None | ❌ | ❌ | **Identical** |
| 7 | one empty | 0.0000 | None | ❌ | ❌ | **Identical** |
| 8 | **hyphen difference** `"cross-site scripting vulnerability"` vs `"cross site scripting"` | **0.2000** | **0.7500** | ✅ | ✅ | **DIFFERENT score** |
| 9 | regular (no hyphen) | 0.7500 | 0.7500 | ✅ | ✅ | **Identical** |
| 10 | **compound diff** `"well-known vulnerability"` vs `"well known issue"` | **0.0000** | **0.5000** | **❌** | **✅** | **DIFFERENT conclusion** |
| 11 | apostrophe preserved | 0.3333 | 0.3333 | ✅ | ✅ | **Identical** |
| 12 | real SSRF example | 0.0000 | None (0.083) | ❌ | ❌ | **Identical** |

---

## 4. Critical Differences

### Case 10: Conclusion mismatch (worst case)
- `finding="well-known vulnerability"`, `golden="well known issue"`
- **Python:** score=0.0, **NO MATCH** (no common tokens: `"well-known"` ≠ `"well"`)
- **Rust:** score=0.5, **MATCH** (Rust splits `"well-known"` into `"well"` and `"known"`, both match)
- **Impact:** Rust would accept this as a match; Python would reject it.

### Cases 4, 8: Different scores (same match conclusion)
- Both implementations agree on match/no-match, but scores differ. This could affect threshold tuning downstream.

---

## 5. Test Files Created/Modified

### Modified
- `crates/crb-judge/src/lib.rs` — Added 10 cross-validation tests (`cv_*`) inside the existing `#[cfg(test)] mod tests` block

### Created (temporary)
- `/tmp/test_jaccard_python.py` — Python test replicating exact `step3_judge_comments.py` logic
- `/tmp/test_jaccard_side_by_side.py` — Side-by-side comparison with Rust-equivalent tokenization
- `/tmp/cross_validation.rs` — Prior draft of Rust tests (was inlined into lib.rs)

---

## 6. Conclusion

**The Rust and Python Jaccard judges are NOT identical — they differ in tokenization strategy, which produces different results for text containing punctuation (hyphens, parentheses, etc.).**

For text without punctuation, scores are identical. The difference only manifests when:
- Words contain hyphens (`cross-site`, `well-known`, `server-side`)
- Words have attached parentheses (`open(url)`, `(cross-site)`)

**Recommendation:** Either:
1. **Align both implementations** to use the same tokenization (preferably whitespace-only split to match Python, since Python's `.split()` is the simpler strategy)
2. **Document the difference** explicitly if the more aggressive Rust tokenization is intentional (e.g., for more lenient matching)

---

## 7. Verification Artifacts

- Rust test run: `cargo test -p crb-judge` → **21 passed, 0 failed**
- Python test run: `python3 /tmp/test_jaccard_side_by_side.py` → 9/12 identical, 3/12 different
- All existing tests continue to pass after adding cross-validation tests
