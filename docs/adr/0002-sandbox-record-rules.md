---
status: accepted
---

# サンドボックスの記録規則

乗り換えは、Prism と同じ path を叩いたまま、create が list に残ることで起きる。公開 URL は契約の `paths` そのままにし、`servers` の host も path も足さない。コレクションの最初の GET が初期記録を作る。example があればそれ、無ければ生成した 3 件。リセットは `POST /__smart_mock/reset` で、契約に同じ path があれば起動しない。リセットのあと、次のコレクション GET が再び初期記録を作る。

POST はクライアントの id を使い、無ければサーバが付ける。整数はそのコレクションの最大 + 1、文字列は UUID。id は POST の必須から外す。POST と PUT は、契約にあって body に無いプロパティを補完する。PATCH は送ったフィールドの型だけを見てマージし、補完しない。記録に無い id への GET、PUT、PATCH、DELETE は 404。一件操作の id は path を正とする。必須以外のクエリは無視し、コレクション全体を返す。コレクション同士は独立している。

認証は、security が指定した場所に値があることだけを見る。空の選択肢 `{}` がある操作は認証を要求しない。仕様違反の body は、理由を一つ持つ JSON である。

## Considered Options

- `servers` の path を公開 URL の前に付ける。Prism に向けている URL が 404 になる。Prism 自身も path を足さない。
- 記録を空から始め、POST まで一覧を `[]` にする。乗り換え直後の画面が、Prism で見えていた example を失う。
- トークンの値を契約の例と照合する。認証サーバの再実装になる。
- リセットをヘッダにする。通常のリクエストに副作用が混ざる。
- id が必須なら無い POST を 400 にする。id 無しの create ができなくなる。
- クエリで記録を絞り込む、またはコレクション間の参照を 400 にする。契約は絞り込みも外部キーも書かない。
- PATCH にも POST と同じ必須を要求する。一つのフィールドを変える更新ができなくなる。

## Consequences

最初の GET は記録を書き換える。契約が id を必須にしていても、id 無しの POST は成功する。`servers.url` に path がある契約は、その path を `paths` に書いておく必要がある。content negotiation、`oneOf`、discriminator は、この規則の外である。
