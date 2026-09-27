---
status: accepted
---

# 顔は OpenAPI 契約のサンドボックス

Prism からの乗り換えは、OpenAPI を渡すと create が list に残り、仕様違反は失敗することで起きる。プロダクトの顔は契約だけにする。TypeScript と JSON 定義は外。記録するのは REST のコレクション（パラメータ無しの path と、末尾が `{id}` の path）だけで、それ以外の操作は記録しない。仕様違反は 400、security の欠落は 401。追加プロパティは契約が `additionalProperties: false` のときだけ拒否する。記録はプロセスとともに消え、起動中に空へ戻す要求を一つ持つ。

## Considered Options

- 三つの入力を残し、Node の要らない単一バイナリとして配る。使う人の側にはすでに Node がある。
- TypeScript の型から CRUD を生やす。TypeServe が同じ仕事をすでに名乗っている。
- 成功した POST をすべて記録する、または文書に印を書かせる。前者は検索やアクションをリソース作成と取り違え、後者は WireMock のシナリオに戻る。
- content negotiation、`oneOf`、discriminator まで Prism と揃えてから記録する。検証器の再実装が記録より先になる。
- 記録をファイルに残す。翌日の GET に昨日の POST が混ざる。

## Consequences

README は OpenAPI だけを案内する。非 REST の操作は記録の外に残る。契約が追加プロパティについて黙っているリクエストは仕様違反にしない。
