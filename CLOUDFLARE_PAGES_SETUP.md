# 七龍珠 HD：Cloudflare Pages 遷移版本

以已驗收的 v60 遊戲核心為基礎，加入 v62 跨網域 10 格存檔還原。
v61 開場文字修正尚未經 iPhone 實測，不包含於此版。

## Cloudflare Pages Git 整合
- GitHub: ghostfung/DBZ-SSD-zhTW
- Production branch: cloudflare-pages-v62
- Framework preset: None
- Build command: 留空
- Root directory: repository root
- Build output directory: web
- Pages Functions: 不需要
- 建議綁定自己的網域: dbz.happy-kt.com（未建立，先測 pages.dev）

請先測試 Cloudflare Pages 的 pages.dev 網址、完整離線安裝、
ROM/IPS 匯入、HD ZIP、.dbzbackup 匯入和存檔還原。
確認無誤後，才把 Vercel 換成輕量搬遷頁。
不同來源的 PWA 不會共用 IndexedDB / OPFS 存檔。
玩家必須在舊站匯出全部存檔，再從新站匯入；HD ZIP 需重新匯入。
無任何 ROM/IPS/HD ZIP 隨網站提供。
