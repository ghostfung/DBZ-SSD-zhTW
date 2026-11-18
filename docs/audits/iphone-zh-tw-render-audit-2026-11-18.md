# iPhone Web 版繁體中文顯示全流程審核

審核日期：2026-11-18  
範圍：`iphone-web-poc`；網頁／WASM 文字呈現流程、官方繁中翻譯表、原專案的 Rust 翻譯渲染器。  
依據：原始碼及使用者提供的實際 iPhone 戰鬥畫面截圖；**未**取得可在 CI 重播的使用者 ROM／戰鬥狀態。  
**結論：目前問題主要出於翻譯「呈現管線」，不是繁中詞庫短缺；不應繼續逐頁加 JavaScript 座標補丁。**

## 1. 翻譯資料驗證

`translations/zh-tw/zh-tw.tsv` 與 `web/zh-tw.tsv` 相同（Git blob SHA 相同）；排除註解與標頭後，共 **1,495** 個唯一鍵值，無重複、無空白譯文。按鍵值分類：

| 類別 | 鍵值數 | 典型顯示 |
| --- | ---: | --- |
| `help:` | 42 | 物品說明／提示 |
| `battle:` | 362 | 戰鬥對話與事件 |
| `str2:` | 567 | 帶角色頭像的對話（含續頁） |
| `str3:` | 178 | 劇情敘述、城鎮與場景文字 |
| `str4:`–`str7:` | 21 | 神龍願望、特殊與系統對話 |
| `string:` | 37 | 主角名稱與固定文字 |
| `unit:` | 104 | 角色／敵人名稱 |
| `attack:` | 69 | 戰鬥招式 |
| `object:` | 40 | 物品名稱 |
| `block:` | 27 | 原生視窗與命令選項 |
| `ending:` | 14 | 結局、工作人員名單 |
| `intro:` | 32 | 開場捲軸文字 |
| `screen:` | 2 | 特別畫面標籤（追擊傷害／卡牌提示） |

**重要：有翻譯鍵值不等於遊戲畫面已被翻譯。** 資料經由 `Translation::parse` 載入，但文字是否真正出現在遊戲畫面，取決於當前的渲染器。

## 2. 路徑審核與缺陷

| 顯示路徑 | 現有來源 | 審核結果 |
| --- | --- | --- |
| 劇情文字（含頭像、續頁） | `WebGame::current_dialogue` → `updateTranslation` → `drawChineseInBox` | **有缺陷**：靠米白色區塊猜位置，沒有使用完整的原版文字格子／換頁模型；未識別時英文可能短暫出現 |
| 戰鬥敘述／短訊 | 同上，另有 `reconcileDialogue` | **有缺陷**：僅特判 `battle:88`／`battle:89`，不是通用的換頁處理 |
| 戰鬥狀態表（Goku、Piccolo、HP/KI/BP） | `visible_text_cells` → `assembleTextRuns` → `drawMenuTranslations` | **有缺陷**：`findDialogueBox` 把狀態表錯認成對話框後，`drawMenuTranslations` 會跳過框內名稱；`Piccolo77` 等相鄰數字／截斷欄位仍可能匹配失敗 |
| 戰鬥命令（戰鬥／卡牌／逃跑／選單） | `block:` 與 `screen_names` | **部分正常**：已實機顯示中文，但目前覆蓋依然是像素塗抹 |
| 招式列表、KI-USE、角色選擇、多層窗口 | `attack:`／`unit:`／`block:` → `drawMenuTranslations` | **有缺陷**：單行掃描與簡單文字配對無法識別跨窗、被框截斷、隔一格及交疊的文字；錯誤遮罩會畫到其他視窗 |
| 地圖／城鎮／商店／物品／狀態／存讀檔 | `help:`、`object:`、`string:`、`block:` | **未完整驗證**：同一套不可靠的文字掃描與繪製機制 |
| 開場／結局／特殊畫面 | `intro:`、`ending:`、`screen:` | **未接入相應原生呈現機制**：一般米白對話框檢測無法覆蓋捲軸、字幕及特殊圖層 |
| Classic 模式 | `WebGame::tick` 僅在 HD 開啟時生成 Rust 文字符格 | **有缺陷**：關閉 HD 2x 時，選單翻譯的 PPU 文字符格更新中止 |
| HD 背景與文字的疊合 | `live_backgrounds.composite` + 瀏覽器 Canvas 中文覆蓋 | **部分正常**：背景已確認可命中；文字與背景仍分屬不同渲染管線 |
| 網頁操作介面 | `web/index.html` | **主要已繁中**；Select、Start、ABXY 是手把按鍵名稱，不應當成遊戲劇情殘留 |

### 可從程式碼直接定位的根因

1. `web/app.mjs` 的 `drawFrame` 對每個米白視窗使用 `findDialogueBox`，無論它是故事框、狀態框、招式視窗或同時存在的多個視窗。
2. `web/screen-text.mjs` 的 `drawMenuTranslations` 會略過與該猜測框相交的字串，因此 `Goku`、`Piccolo` 即使已在字典內，仍可能完全不畫。
3. `web/chinese-overlay.mjs` 的 `maskPendingDialogue` 亦取用同一個猜測框；當不存在有效的對話鍵值時，可能遮蓋並非對話的欄位。
4. `assembleTextRuns` 以每行 x 座標相差 8 像素合併字元，忽略實際視窗的分隔、圖層及欄位。簡單前綴替換對 `Piccolo77`、`Piccol` 或被視窗框裁切的名稱均不可靠。
5. `reconcileDialogue` 僅為指定英文句子提供例外，無法普遍處理 `on_screen` 回傳的 `stopped`、多頁、定時訊息、名字插槽與跨視窗覆蓋。
6. `crates/ssd-web/src/lib.rs` 現有 `tracked_renderer` 啟用了 `set_text_capture_only(true)`；Rust 只擷取英文字元位置，不使用原版已有的完整繁中繪製器。

## 3. 原專案已有、Web 版卻未整合的功能

`crates/ssd-hd/src/translate.rs` 已有狀態式 `Translator` 與 `Overlay`：

- `draw_names` 處理選單／物品／招式、被欄位截斷的名字、緊鄰的數值及游標；`Overlay::name` 亦處理安全前綴匹配。
- `draw_pages`、`typed_presence`、`presence_in_window` 依照 PPU 文字格及引擎進度辨識當下真正顯示的頁面，處理先前頁面被新視窗蓋住等情況。
- `draw_intro`、`draw_chase_damage`、`draw_rules` 涵蓋一般對話以外的顯示。
- `crates/ssd-hd/src/world/wide.rs` 的 `WideRenderer::set_overlay`／`set_text_screen` 已使用上述機制，不依靠猜測米白色矩形；並有原作者的單元測試。

**因此建議停止擴充 `findDialogueBox`／`maskPendingDialogue` 這套臨時系統，以同一個 Rust 畫面狀態與翻譯輸出為權威來源。** 必須先驗證在 iPhone WASM 版的適用性、中文字型資源與渲染成本，不能未驗證便直接替換已可用的 HD 背景渲染。

## 4. 下階段一次驗收的標準

改動不應只以「GitHub Actions 綠燈」或「某張截圖中文正常」判定成功。應使用可重播測試資料（不需提交或公開玩家的 ROM），至少涵蓋：

1. 劇情：一般對話、帶肖像、三行文字、換頁、快速連按、延時事件；無英文閃爍、無錯頁或雙語重疊。
2. 戰鬥提示：遭遇、回合訊息、技能文字、狀態通知、退出與重入。
3. 戰鬥狀態：悟空、比克、其他角色；HP/KI/BP、變動數值、名字被裁切。
4. 戰鬥多層選單：指令／角色選擇／手動自動／招式表及 KI-USE，框、游標、卡牌與圖層不可被遮蓋。
5. 地圖／城鎮／商店／道具／狀態／存讀檔／文字速度。
6. 開場、結局、特殊字幕和提示。
7. Classic 與 HD 2x 的文字策略；HD 背景實際像素、音效及效能均不回退。

**實測限制：** 原始碼與繁中資料可完整靜態審核，但沒有可在 CI 執行的遊戲 ROM／所有場景狀態，無法宣稱所有遊戲畫面已經跑完；使用者不應被要求重複逐個場景手動尋錯。先補自動化與同一權威渲染管線，再做少量實機總驗收。

## 5. 風險與不應再做的事情

- 不再按截圖個別硬編座標、不再只特判 `battle:88` 類型的單句文字、不以「米白色矩形」等同「對話框」。
- 不修改 ROM／IPS，不把 ROM、個人存檔或 HD 素材上傳；中文資產只用原作者授權檔案。
- 新舊渲染管線須能比較和回退；效能下降或 HUD 遭遮蔽均視為未通過。
