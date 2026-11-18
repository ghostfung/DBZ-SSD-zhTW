# iPhone Safari WebAssembly POC (未完成遊戲移植)

這個測試分支基於 `lming888/DBZ-SSD-zhTW`，目標是讓《超級賽亞人傳說》在 iPhone Safari **本機**執行，不需串流。原有 Windows/Android 發行程式不受這個獨立原型影響。

## 現況

- `crates/ssd-web/`：新的 WebAssembly bridge，嘗試用 `ssd-oracle::Machine` 輸出 SNES Classic RGBA 畫面，尚未通過 GitHub Actions 真正編譯。
- `web/`：本機 ROM、IPS 匯入、觸控按鍵、Canvas、存檔匯出/匯入、PWA shell。
- `.github/workflows/web-poc.yml`：測試原型是否能交叉編譯為 `wasm32-unknown-unknown`，成功才產生網頁 Artifact。
- **現階段沒有 HD 繁體中文渲染，也沒有 Web Audio，請勿稱為可玩的 HD 繁中版。**
- 目前暫時使用 `NullAudio`；PWA 不能保證 iPhone 離線存檔長期保留。

## GitHub Actions 測試

1. 開啟 Fork 的 [Actions](https://github.com/ghostfung/DBZ-SSD-zhTW/actions)，選擇 **DBZ iPhone Web POC (WASM)**。
2. GitHub Actions 啟用後，推送 `iphone-web-poc` 分支會自動啟動測試；如流程不自動執行，檢查 Fork 的 Actions 設定。
3. 先確認 **Check SNES core WASM compatibility** 能否通過，再依失敗的 Rust 編譯訊息修正依賴或平台 API。
4. 全部步驟成功後下載 `dbz-ssd-web-poc` Artifact，再放到 HTTPS 靜態主機測試 iPhone。**尚未成功編譯前不要部署 Vercel**。

## 安全與版權

未加入任何商業 ROM、IPS、HD 素材包或著作權字型。遊戲檔案必須使用者自行於瀏覽器本機匯入，網頁不向伺服器上傳。原專案授權與素材授權需分別遵守，勿直接公開散布 ROM 或未獲授權素材。
