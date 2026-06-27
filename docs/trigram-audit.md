# Trigram SQL Pattern Audit

Date: 2026-06-27
Phase: 5 (Trigram Ranking Adoption)

## Summary

Audited the entire `synapse-storage/src/` codebase for trigram/similarity/ILIKE SQL patterns.
Found patterns in 7 files, documented below with adoption recommendations.

---

## 1. Established TrigramRanking Consumers

These files already use `TrigramRanking` — no changes needed.

### 1a. `synapse-storage/src/user.rs` (lines ~762–818)
- Method: `search_users`
- Columns: `username`, `user_id`, `displayname`
- Uses `TrigramRanking::column_match_subquery()` with three rankers UNION ALL'd
- Correctly calls `escape_like_pattern()` before binding

### 1b. `synapse-storage/src/space.rs` (lines ~640–760)
- Method: `search_spaces`
- Columns: `name`, `topic`
- Uses `TrigramRanking::match_priority_case()`, `similarity_expr()`, `where_clause()`, and `column_match_subquery()`
- Two code paths: with and without visibility filtering
- Correctly calls `escape_like_pattern()` before binding

---

## 2. Recently Adopted — TrigramRanking Applied in This Phase

### 2a. `synapse-storage/src/search_index.rs` (line ~116)

**Before:**
```sql
WHERE (content ILIKE $1 OR content % $2)
```
With bindings: `$1 = "%term%"`, `$2 = term`

**After:**
```sql
WHERE ({TrigramRanking::where_clause()})
```
Which expands to:
```sql
WHERE (content ILIKE $1 ESCAPE '\'
   OR content ILIKE $2 ESCAPE '\'
   OR content ILIKE $3 ESCAPE '\'
   OR (char_length($4) >= 3 AND content % $4))
```
With bindings: `$1 = exact`, `$2 = prefix%`, `$3 = %contains%`, `$4 = raw_term`

Changes:
- Added `ESCAPE '\'` for proper special-character handling
- Added exact-match and prefix-match ILIKE conditions (redundant with contains, but harmless and consistent)
- Added `char_length($4) >= 3` guard on trigram operator (prevents meaningless trigram on short strings)
- Escape function (`escape_like_pattern`) now escapes `\`, `%`, and `_` before binding
- Parameter positions shifted: cursor params moved from `$3/$4` to `$5/$6`, limit from `$5` to `$7` (cursor) / from `$3` to `$5` (non-cursor)

---

## 3. Complex Patterns — Not Adoptable Without API Expansion

### 3a. `synapse-storage/src/user.rs` (lines ~1050–1179)
- Method: Second search path with CUSTOM weighted scoring
- Columns: `username`, `displayname`, `email`, `user_id`
- Pattern: Each column has its own CASE expression with domain-specific weights (1000 for username, 950 for displayname, 900 for email, 875 for user_id)
- Uses `NOT $4` boolean gating to conditionally disable fuzzy/prefix matching
- Aggregated via UNION ALL with `MAX(rank_score)` and `MIN(match_category)`
- **Why not adoptable**: TrigramRanking's generic priority (0-3) cannot represent domain-specific weights (480-1000 scale).
  Adopting TrigramRanking here would require:
  - An API extension to accept custom score weights per rank level
  - Support for conditional gating (`NOT $4`)
  - Support for scaled similarity scores (`ROUND(similarity(x, $5) * 100)::INTEGER`)

### 3b. `synapse-storage/src/thread.rs` (lines ~920–995)
- Method: `search_threads`
- Columns: `e.content->>'body'` and `latest_reply.latest_content->>'body'` (JSONB extraction)
- Combines FTS (`ts_rank_cd`, `to_tsvector`, `plainto_tsquery`) AND trigram (`similarity`, `%` operator) in a single `GREATEST()` expression
- Uses `GREATEST(ts_rank_cd, similarity, ts_rank_cd, similarity)` for combined ranking
- WHERE clause mixes ILIKE with `'%' || $2 || '%'` concatenation (non-standard parameter injection) and `%` operator
- **Why not adoptable**: TrigramRanking does not support:
  - JSONB column access via `->>'body'`
  - Full-text search functions (`ts_rank_cd`, `to_tsvector`, `plainto_tsquery`)
  - Combined FTS + trigram ranking in `GREATEST()`
  - Non-standard `'%' || $2 || '%'` ILIKE pattern (uses `$n` concatenation, not direct binding)
  - LATERAL JOIN references in column expressions
  Significant API expansion would be required (at minimum: jsonb_path parameter, FTS function injection,
  custom ranking expression injection).

### 3c. `synapse-storage/src/event/mod.rs` (lines ~1185–1248)
- Method: Full-text search on events
- Uses pure PostgreSQL FTS: `to_tsvector('english', e.content) @@ plainto_tsquery('english', $2)`
- Ranking with `ts_rank(to_tsvector(...), plainto_tsquery(...))`
- Cursor-based pagination on rank + origin_server_ts + event_id
- No trigram usage at all
- **Why not adoptable**: Pure FTS, no trigram patterns. TrigramRanking is irrelevant here.

---

## 4. Simple ILIKE Patterns — Not Adoptable (No Trigram)

These files use simple ILIKE filtering without trigram `%` operator. They could potentially benefit from
adding trigram support, but as pure-filters they don't match TrigramRanking's current API.

### 4a. `synapse-storage/src/room/admin.rs` (lines ~480–510)
- Uses bare ILIKE on columns: `r.name`, `r.topic`, `r.canonical_alias`, `r.room_id`
- Trigram `%` operator only on `r.name` and `r.canonical_alias` when `term.len() >= 3`
- Dynamic SQL built via `query.push()` and `query.push_bind()`
- No ranking/ordering by relevance — just filtering
- **Verdict**: Could be consolidated but requires a non-subquery API from TrigramRanking (just the WHERE
  clause, no subquery wrapping). The dynamic SQL construction also differs from TrigramRanking's
  string-returning approach.

### 4b. `synapse-storage/src/sliding_sync.rs` (line ~684)
- Single `COALESCE(name, '') ILIKE` filter
- No trigram, no ranking
- **Verdict**: Not relevant to TrigramRanking.

---

## 5. Unused Patterns

- No trigram/similarity patterns found in `src/web/routes/`
- All ILIKE/trigram patterns are confined to `synapse-storage/src/`

## Recommendations

1. **search_index.rs**: Already adopted in this phase. [DONE]

2. **user.rs (second search path)**: Defer. The custom weighted scoring (480-1000) and conditional
   gating (`NOT $4`) would require a new `WeightedTrigramRanking` abstraction or an API that accepts
   custom score tuples.

3. **thread.rs**: Defer. The JSONB + FTS + trigram hybrid is a distinct pattern. A future
   `HybridSearchRanking` abstraction could unify this, but it's out of scope for the current
   TrigramRanking API.

4. **room/admin.rs**: Low priority. The dynamic SQL construction pattern differs significantly from
   TrigramRanking's approach. If TrigramRanking gains a `where_clause_only()` method that returns a
   simpler clause (no subquery), this caller could benefit.

5. **event/mod.rs**: No action. Pure FTS is a different concern.
