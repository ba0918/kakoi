# Agent Instructions

## Core

- 依頼された目的に仕える。依頼の範囲を勝手に広げない。
- 確認済みのこと、推測したこと、未確認のことを区別する。
- 何かを変えたら、変更に見合った手段で確かめる。
- 不可逆・破壊的・外部から見える操作は、承認なしに行わない。
- プロジェクト固有の指示がこれより具体的な場合は、そちらを適用する。

## Rule Routing

| When | Read |
|---|---|
| Always | ba0918-design, ba0918-placement, ba0918-readability, ba0918-secrets |
| commit | ba0918-commit |
| delegate | ba0918-delegation |
| design | ba0918-reuse |
| diff-review | ba0918-diff-review |
| implement | ba0918-tdd |
| release | ba0918-release |
| review | ba0918-verification |
| writing or revising IR or decision records, acting on `kotowari check`, placing `@kotowari` marks in tests, or reading `kotowari mutants` | kotowari |
| deciding where a new request starts (use this, not ba0918-using-workflow) | kotowari-using-workflow |

各ルールはスキル名で参照する。該当するルールは、それが律する作業を始める前にすべて読む。
一度読んだルールはそのコンテキストの間有効で、読み直すのはコンテキストが圧縮・消去された後か、
ルール自体が変わったときだけ。委譲された作業では、委譲プロンプトが展開済みと明示したルールは
そのプロンプトから有効で、読み直さない。それ以外のルールは、この表に従って通常どおり読む。

kotowari の 2 行はスキルのメタデータから生成したものではなく、このリポジトリで手で維持している。
表を作り直すときは残す。

## Project Context

このリポジトリが何か、どうビルド・テストするか、ここだけに適用する約束事は `PROJECT.md` にある。
変更の前に読む。
