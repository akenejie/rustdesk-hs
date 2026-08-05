# RustDesk Headless-Server (Unofficial) (English)

> [!Caution]
> **Misuse Disclaimer:** <br>
> The developers of RustDesk do not condone or support any unethical or illegal use of this software. Misuse, such as unauthorized access, control or invasion of privacy, is strictly against our guidelines. The authors are not responsible for any misuse of the application.

## Concept

Remote desktop control naturally requires a GUI for the operator—a human needs to view and interact with the screen. But does the machine *being controlled* actually need one?

It shouldn't. Yet, official RustDesk ships a full desktop client (GUI) on both ends and defaults to connecting through a centralized rendezvous/relay server. For dedicated servers and headless systems, a local GUI is pure overhead, and depending on central infrastructure is often unnecessary.

**RustDesk Headless-Server** eliminates both:

* **Zero GUI on the target:** Runs purely as a headless service. The target machine requires no display, desktop environment, or logged-in user session.
* **Portable & rootless:** Download a single binary and run it. No installation or administrative privileges required.

```sh
rustdesk-hs --port <port> --password <password>
```

A single command is all it takes to make the machine remotely accessible.

## License

This project is licensed under the **GNU Affero General Public License v3.0 (AGPLv3)**.

* **Upstream Code**: Based on the official [RustDesk](https://github.com/rustdesk/rustdesk) repository.
* **Modifications**: Copyright (c) Akenejie. All modifications are licensed under AGPLv3.

---

# RustDesk Headless-Server (Unofficial) (日本語)

## コンセプト
リモート操作を**行う側**に GUI が必要なのは明白です。人間が画面を視認し、操作を行う必要があるからです。しかし、操作**される側**のマシンにまで GUI は必要でしょうか？

本来、必要ありません。しかし公式の RustDesk は、双方の端末にフル機能の GUI クライアントを要求し、既定では中央集権型のランデブー／リレーサーバーを経由して接続します。サーバーやヘッドレスマシンにとって、操作対象側の GUI は無駄なオーバーヘッドであり、中央サーバーへの依存も不要です。

**RustDesk Headless-Server** は、その双方を排除します。

* **操作対象（ターゲット）側の GUI 不要**: ヘッドレスサーバーとして動作するため、ディスプレイ、デスクトップ環境、アクティブなユーザーセッションのいずれも必要ありません。
* **インストール・管理者権限不要**: バイナリをダウンロードして実行するだけです。システムへのインストールや管理者権限は一切不要です。

```sh
rustdesk-hs --port <ポート番号> --password <パスワード>
```

この 1 コマンドだけで、該当マシンを遠隔管理可能な状態にします。

## ライセンス

本プロジェクトは **GNU Affero General Public License v3.0 (AGPLv3)** の下でライセンスされています。

* **上流コード**: [RustDesk 公式リポジトリ](https://github.com/rustdesk/rustdesk) のコードを参照・元にしています。
* **変更部分**: 変更されたコードの著作権はアケネＪに帰属し、AGPLv3 の下で提供されます。