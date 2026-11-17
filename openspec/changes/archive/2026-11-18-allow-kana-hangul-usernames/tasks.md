# Tasks

## 1. Extend non-federated username validation

- [x] 1.1 Add Japanese hiragana, katakana, and Korean Hangul to the non-federated username character policy without changing federated validation.
- [x] 1.2 Add validator coverage for accepted kana and Hangul names, existing accepted Chinese names, and rejected out-of-policy characters.

## 2. Verify account workflows

- [x] 2.1 Add coverage that registration and username changes accept kana and Hangul names on non-federated sites and continue to reject them under federated actor-name rules.
- [x] 2.2 Add login coverage showing users can authenticate with kana and Hangul usernames on their owning non-federated site.
