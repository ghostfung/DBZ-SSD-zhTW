# Vercel 舊站：極輕量搬遷頁（未部署）

分支 `vercel-migration-bridge-v1` **只有輕量頁面和純 JS**。
沒有 18 MB WASM、遊戲核心、ROM、IPS、HD 美術包或自動離線整包下載。

原 v60 的 Service Worker 會在瀏覽器再次檢查更新後，
取代成這個輕量 Worker。新的 Worker 在 install 僅儲存四個小檔案，
之後導航會進入新搬遷頁；不刪除 IndexedDB 或舊版快取。
**仍在玩舊版的玩家不會強制中斷**。離線玩家亦未必會收到更新。

保留 10 格 IndexedDB 原始格式；一鍵匯出 `.dbzbackup`。
Cloudflare 新版從同一格式匯入，要求相同 ROM/IPS。
強烈建議確認 Cloudflare 新站能遊玩、能匯入存檔、已綁定
`dbz.happy-kt.com` 後，再短暫恢復 Vercel 並部署此分支
至正式專案。不可以在 Cloudflare 網址未驗證之前發布舊站公告。

舊版 PWA 不可能保證所有玩家立即切換，iOS 決定何時更新 Service Worker。
