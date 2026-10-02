# Writing SAFETY comments

Every `unsafe` block, `unsafe impl`, and `unsafe fn` in this workspace carries a
`// SAFETY:` comment that is a proof sketch, not a restatement of the operation.
A reader must be able to check the reasoning without re-deriving it.

The common pattern is obligation-first:

1. **Obligation:** the precondition(s) the unsafe operation requires, stated
   precisely enough to check against the API being called (for example:
   "`new_unchecked` requires a non-zero value", "`from_raw_parts` requires …
   all of those bytes properly initialized").
2. **Facts:** the local facts that discharge each obligation — guards and their
   values, where a value came from, type-level properties (validity of an
   all-zero value, absence of padding), lifetimes, exclusivity. Cite the
   dominating check rather than saying "checked above" when the guard is not
   lexically adjacent.
3. **Result:** any postcondition the next unsafe operation or a `Drop` impl
   relies on, when it is not obvious (for example: which restored value a guard
   will restore).

Rules of thumb:

- One SAFETY comment per unsafe block, adjacent to it; when a block contains
  several operations, the comment covers their obligations in program order.
- Prefer the actual contract wording over folklore ("POSIX standard function"
  says nothing; "takes an arbitrary integer fd and reports failure by return
  value" is checkable).
- State the property that makes a zeroed or converted value valid (integer-only
  fields, no padding, guard on the arm); do not rely on implementation details
  of dependencies that are not part of their contract (for example the internal
  representation of `ThreadId`).
- For FFI: name the foreign function's actual preconditions and the error
  reporting convention, and note that return values are checked before any
  output is consumed.
- Panics matter too: an `unwrap` inside an unsafe block gets its justification
  in the same comment.
- If the safety argument depends on how a value is produced elsewhere (a guard
  storing an original to restore), say where that happens; the comment should
  read as a closed loop with its counterpart.
