# iPhone HD 第十一階段：素材覆蓋盤點（程式碼審查）

## 比對基準

- 目前 iPhone 工作分支：`iphone-hd-integration-batch`，基準 `bd9fbb6562f5412c77054a2190aa6322fe7bee98`。
- 素材格式來源：此倉庫 `crates/ssd-hdpack/src/import.rs`，對照上游 `JackRushante/DBZ-Super-Saiya-Densetsu-Rust` 的 `crates/ssd-hd/src/art.rs` 與 `menu_art.rs`。
- 盤點針對官方 `hd-pack.zip` v0.3.9 的索引結構，**不把已索引、已載入或本幀替換當作全遊戲覆蓋率**。

## 程式支援狀態

| 官方索引 | Web 支援 | 檢查與限制 |
|---|---|---|
| `bg/index.json` | 部分支援 | PPU/VRAM 來源鍵值；逐幀背景合成 |
| `class1/index.json` | 部分支援 | ROM 4bpp 圖塊、調色盤、可見頭像核對 |
| `class2/index.json` | 部分支援 | 戰鬥人物與特效的 OAM 鍵值、尺寸／調色盤比對；`luma` 不可直接套用 RGB；外擴輪廓未完全支援 |
| `class3/index.json` | 實驗支援（素材包可選） | 地圖 NPC／物件的 OAM 來源比對 |
| `cards/index.json: tiles` | 部分支援 | ROM 2bpp `base`／`roles`；保留穩定、完整圖塊選取 |
| `cards/index.json: tips` | **未支援** | 原作者以額外選單條件顯示 TIP 圖像，不能直接當成卡牌字形替換 |
| `maps/index.json` | 部分支援（素材包可選） | ROM 指標＋PPU BG2；僅在確定來源時讀圖，另有低優先預讀 |
| `mode7/index.json` | 實驗支援 | VRAM／調色盤核對和 SNES 掃描線矩陣取樣 |
| `variants/index.json` | **未支援** | `art`、`when`、`margin`、`file` 需與當前人物姿勢及 Work RAM 條件共同核對 |
| 獨立 HD 選單圖片 | **未完整支援** | 上游透過原始圖塊組合與配色辨認，並非可用檔名猜測 |
| 16:9／人物外擴輪廓 | **未完成** | 屬畫面引擎功能，不等於索引檔漏讀 |

## 本次查到的載入競態

過往 `web/hd-import-ui.mjs::loadPortraitIndex` 在成功讀完 class1 時，誤送其他四類索引的 `catalog:null` 事件。因索引讀取是並行進行，成功匯入的卡牌、地圖、特效及 Mode 7 可能在頭像最後載入時被清空。第十一階段將事件範圍收窄成只通知自己的 `hd-portrait-catalog-ready`；其他模組只在 ZIP 真正更換或清除時重設。

## 可驗收的新增工具

在遊戲「HD 設定與效能分析（點擊展開）」內：

1. 匯入並驗證玩家自己的官方 `hd-pack.zip`。
2. 點「分析本機 HD 素材覆蓋」，讀取官方 8 類索引、核對相關圖片存在性（**不解壓圖片**）。
3. 報告註明不存在索引、格式不合法、缺檔、`luma`、未支援類別等原因；只顯示少量代表性的缺檔路徑。
4. 正在玩遊戲時可附上**目前一個影格**的匹配／已就緒筆數及替換像素，不會追蹤完整遊戲流程。
5. 按「下載盤點 JSON」把報告留在 iPhone 本機，方便傳回分析。沒有任何自動上傳。

## 驗收條件

- 卡牌在戰鬥選卡時仍然穩定，不重現隨機閃爍。
- `class1` 晚於其他索引完成時，不會清空已載入的地圖、卡牌、特效、Mode 7。
- 報告不含玩家 ROM／IPS／存檔，亦不宣稱覆蓋率百分比。
- 沒有確認來源與角色條件的特效／變身姿勢必須保持原版，不能猜測替換。
- 報告可在 HD 診斷區收合、不阻礙 iPhone 控制器使用。

下一個功能實作的優先順序，應根據玩家回傳的來源缺口和實際遊戲影格診斷確定；不能單憑索引中圖片總數決定。
