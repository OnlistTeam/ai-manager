/*
 * Four languages on one page.
 *
 * The page is a single document rather than four, because it exists for one
 * reason: the application hard-codes `https://aimanager.tools/download` as the
 * fallback a user reaches when an automatic update fails. That address has to
 * resolve to something useful in whatever language the visitor reads, which is
 * a translation problem, not a site-structure problem.
 *
 * The static English copy lives in `index.html` so the page still says
 * something without JavaScript, and is captured on load as the fallback for
 * every language. The `en` block below is therefore short: it holds only the
 * strings `download.js` builds at run time, which have nowhere in the document
 * to live.
 *
 * Dictionary values may contain a little markup, and are applied with
 * `innerHTML`. That is safe here and only here: every string below is a literal
 * in this first-party file, and nothing the visitor or the network supplies
 * ever reaches it.
 */

const LANGUAGES = ["en", "zh", "zh-TW", "ja"];
const FALLBACK = "en";
const STORAGE_KEY = "aimanager.language";

/** What `toLocaleDateString` should be handed for each of our languages. */
const DATE_LOCALES = {
  en: "en",
  zh: "zh-CN",
  "zh-TW": "zh-TW",
  ja: "ja-JP",
};

const COPY = {
  en: {
    "dl.eyebrow": "Download AI Manager",
    "dl.eyebrowPlatforms": "Choose your system",
    "dl.allHeading": "All platforms",
    "dl.choosePlatform": "Pick a platform to see its installers and checksums.",
    "dl.platformsLabel": "Platforms",
    "dl.tabMac": "macOS",
    "dl.tabWindows": "Windows",
    "dl.tabLinux": "Linux",
    "dl.versionLine": "Version {version}, published {date}",
    "dl.releaseNotes": "release notes",
    "dl.mobile":
      "AI Manager is a desktop application. Open this page on the Windows, macOS, or Linux computer you want to install it on.",
    "dl.macArm": "Download for Apple Silicon",
    "dl.macIntel": "Download for Intel",
    "dl.windows": "Download for Windows",
    "dl.linux": "Download the AppImage",
    "dl.macArmName": "Apple Silicon installer (.dmg)",
    "dl.macIntelName": "Intel installer (.dmg)",
    "dl.windowsName": "Windows installer (.msi)",
    "dl.appImageName": "AppImage",
    "dl.debName": "Deb package",
    "dl.macosCardTitle": "Installer image (.dmg)",
    "dl.macosCardBody":
      "Open the disk image and drag AI Manager to Applications.",
    "dl.windowsCardTitle": "Windows installer (.msi)",
    "dl.windowsCardBody": "Run the installer and follow the prompts.",
    "dl.linuxCardTitle": "Linux packages",
    "dl.linuxCardBody":
      "Choose AppImage for most distributions, or deb for Ubuntu and Debian.",
    "dl.archHintBoth":
      'Not sure which? Apple menu › About This Mac. A chip named "Apple M-something" is Apple Silicon.',
    "dl.archHintOther": "On an Intel Mac instead? {link}.",
    "dl.archHintOtherLink": "Download the Intel build",
    "dl.archHintIsIntel": "On an Apple Silicon Mac instead? {link}.",
    "dl.archHintIsIntelLink": "Download the Apple Silicon build",
    "dl.download": "Download",
    "dl.checksum": "SHA-256",
    "dl.hashCopied": "Copied",
    "dl.source": "Downloads are served from {mirror}. {link}.",
    "dl.sourceLink": "Read release notes",
    "dl.note.macos-arm64": "For Macs with an M-series chip",
    "dl.note.macos-x64": "For Macs with an Intel chip",
    "dl.note.windows-x64": "The standard Windows installer",
    "dl.note.linux-appimage":
      "Runs on most distributions. Pick this if unsure.",
    "dl.note.linux-deb": "For Ubuntu and Debian",
  },

  zh: {
    "meta.title": "下载 AI Manager",
    "meta.description":
      "AI Manager 在你自己的电脑上安装、更新、配置和修复 AI 编程命令行工具。免费开源，AGPL-3.0 协议。",
    "lang.label": "语言",
    "head.tagline": "一个安静的 AI 编程工具桌面管家",
    "head.blurb":
      "安装、更新、配置和修复 Claude Code、Codex、OpenCode 以及另外七个命令行工具，不必碰 shell 配置文件、<code>PATH</code>，也不必手改配置。所有数据都留在你自己的电脑上。",

    "dl.eyebrow": "下载 AI Manager",
    "dl.eyebrowPlatforms": "选择你的系统",
    "dl.heading": "下载",
    "dl.loading": "正在查询最新版本…",
    "dl.choosePlatform": "选择平台，查看安装包和校验和。",
    "dl.platformsLabel": "平台",
    "dl.tabMac": "macOS",
    "dl.tabWindows": "Windows",
    "dl.tabLinux": "Linux",
    "dl.allHeading": "全部平台",
    "dl.versionLine": "版本 {version}，发布于 {date}",
    "dl.releaseNotes": "更新说明",
    "dl.mobile":
      "AI Manager 是桌面应用。请在你想安装它的 Windows、macOS 或 Linux 电脑上打开这个页面。",
    "dl.macArm": "下载 Apple Silicon 版",
    "dl.macIntel": "下载 Intel 版",
    "dl.windows": "下载 Windows 版",
    "dl.linux": "下载 AppImage",
    "dl.macArmName": "Apple Silicon 安装包（.dmg）",
    "dl.macIntelName": "Intel 安装包（.dmg）",
    "dl.windowsName": "Windows 安装程序（.msi）",
    "dl.appImageName": "AppImage",
    "dl.debName": "Deb 安装包",
    "dl.macosCardTitle": "安装镜像（.dmg）",
    "dl.macosCardBody":
      "打开磁盘映像，然后把 AI Manager 拖到 Applications 文件夹。",
    "dl.windowsCardTitle": "Windows 安装程序（.msi）",
    "dl.windowsCardBody": "运行安装程序，按提示完成安装。",
    "dl.linuxCardTitle": "Linux 安装包",
    "dl.linuxCardBody": "大多数发行版选 AppImage，Ubuntu 和 Debian 可选 deb。",
    "dl.archHintBoth":
      "不确定是哪一个？点左上角苹果菜单 › 关于本机。芯片名字里带“Apple M”的就是 Apple Silicon。",
    "dl.archHintOther": "用的是 Intel Mac？{link}。",
    "dl.archHintOtherLink": "下载 Intel 版",
    "dl.archHintIsIntel": "用的是 Apple Silicon Mac？{link}。",
    "dl.archHintIsIntelLink": "下载 Apple Silicon 版",
    "dl.download": "下载",
    "dl.checksum": "SHA-256",
    "dl.hashCopied": "已复制",
    "dl.source": "下载由 {mirror} 提供。{link}。",
    "dl.sourceLink": "查看更新说明",
    "dl.note.macos-arm64": "适用于 M 系列芯片的 Mac",
    "dl.note.macos-x64": "适用于 Intel 芯片的 Mac",
    "dl.note.windows-x64": "标准的 Windows 安装程序",
    "dl.note.linux-appimage": "大多数发行版通用，不确定就选这个",
    "dl.note.linux-deb": "适用于 Ubuntu 和 Debian",
    "dl.fallback":
      '下载列表没能加载出来。所有安装包在 <a href="https://github.com/OnlistTeam/ai-manager/releases">GitHub Releases 页面</a>上也有一份。',
    "dl.prerelease":
      "这是一个<strong>预发布版本</strong>。它和正式版用同样的方式签名与校验，但还没有在每个平台上走完干净机器验证。请预期会有粗糙之处，并且给你在意的东西留一份备份。",

    "shot.alt": "AI 管家窗口，列出 Claude Code、Codex CLI、OpenCode 等工具的版本和更新按钮。",
    "shot.caption": "电脑上的每一个 AI 编程工具，都在一个窗口里：装了哪些、哪些该更新、每个正连着哪个服务。",
    "verify.heading": "安装之前",
    "verify.summary": "签名、首次运行的提示，以及怎么校验下载的文件。",
    "verify.macos.title": "macOS",
    "verify.macos.body":
      "应用和磁盘映像都用 Apple Developer ID 签名、经 Apple 公证并已 staple，所以 macOS 打开它们不会报警告。如果 Gatekeeper 报错，说明文件在发布之后被改动过，不要打开。",
    "verify.windows.title": "Windows",
    "verify.windows.lead":
      "安装程序<strong>没有 Authenticode 签名</strong>，所以首次运行时 SmartScreen 会显示“未知发布者”警告。这是预期内的。购买代码签名证书是一个经过权衡的否决，取而代之的完整性证据是公布的 SHA-256，值得核对一下：",
    "verify.windows.compare":
      "把结果和安装程序旁边发布的 <code>.msi.sha256</code> 文件比对。",
    "verify.linux.title": "Linux",
    "verify.linux.lead":
      "Linux 没有跨发行版的平台签名机制，所以这里同样由公布的校验和来证明下载没问题：",
    "verify.linux.chmod":
      "AppImage 需要先加上可执行权限（<code>chmod +x</code>）才能运行。",
    "verify.updates.title": "更新",
    "verify.updates.body":
      "装好之后你就不需要这个页面了。AI Manager 会在后台检查更新，用编译进应用的公钥校验 minisign 签名，校验不通过的一律不下载。安装仍然由你决定：应用会一直等到你按下重启。",

    "footer.source": "源代码",
    "footer.releases": "全部版本",
    "footer.security": "报告安全漏洞",
    "footer.notices": "第三方声明",
    "footer.licence":
      'AI Manager 是自由软件，采用 <a href="https://github.com/OnlistTeam/ai-manager/blob/main/LICENSE">GNU AGPL-3.0-or-later</a> 协议。它所依赖的开源工作的署名在<a href="https://github.com/OnlistTeam/ai-manager/blob/main/THIRD_PARTY_NOTICES.md">第三方声明</a>里。',
    "footer.privacy":
      "没有分析，没有遥测。这个页面只加载它自己的文件，和一份来自我们自己主机的版本清单。",
  },

  "zh-TW": {
    "meta.title": "下載 AI Manager",
    "meta.description":
      "AI Manager 在你自己的電腦上安裝、更新、設定和修復 AI 程式設計命令列工具。免費開源，AGPL-3.0 授權。",
    "lang.label": "語言",
    "head.tagline": "一個安靜的 AI 程式設計工具桌面管家",
    "head.blurb":
      "安裝、更新、設定和修復 Claude Code、Codex、OpenCode 以及另外七個命令列工具，不必碰 shell 設定檔、<code>PATH</code>，也不必手改設定。所有資料都留在你自己的電腦上。",

    "dl.eyebrow": "下載 AI Manager",
    "dl.eyebrowPlatforms": "選擇你的系統",
    "dl.heading": "下載",
    "dl.loading": "正在查詢最新版本…",
    "dl.choosePlatform": "選擇平台，查看安裝檔與驗證碼。",
    "dl.platformsLabel": "平台",
    "dl.tabMac": "macOS",
    "dl.tabWindows": "Windows",
    "dl.tabLinux": "Linux",
    "dl.allHeading": "全部平台",
    "dl.versionLine": "版本 {version}，發布於 {date}",
    "dl.releaseNotes": "更新說明",
    "dl.mobile":
      "AI Manager 是桌面應用程式。請在你想安裝它的 Windows、macOS 或 Linux 電腦上開啟這個頁面。",
    "dl.macArm": "下載 Apple Silicon 版",
    "dl.macIntel": "下載 Intel 版",
    "dl.windows": "下載 Windows 版",
    "dl.linux": "下載 AppImage",
    "dl.macArmName": "Apple Silicon 安裝檔（.dmg）",
    "dl.macIntelName": "Intel 安裝檔（.dmg）",
    "dl.windowsName": "Windows 安裝程式（.msi）",
    "dl.appImageName": "AppImage",
    "dl.debName": "Deb 安裝檔",
    "dl.macosCardTitle": "安裝映像檔（.dmg）",
    "dl.macosCardBody":
      "開啟磁碟映像檔，然後將 AI Manager 拖到 Applications 資料夾。",
    "dl.windowsCardTitle": "Windows 安裝程式（.msi）",
    "dl.windowsCardBody": "執行安裝程式，依照提示完成安裝。",
    "dl.linuxCardTitle": "Linux 安裝檔",
    "dl.linuxCardBody": "大多數發行版選 AppImage，Ubuntu 與 Debian 可選 deb。",
    "dl.archHintBoth":
      "不確定是哪一個？點左上角蘋果選單 › 關於這台 Mac。晶片名稱裡有「Apple M」的就是 Apple Silicon。",
    "dl.archHintOther": "用的是 Intel Mac？{link}。",
    "dl.archHintOtherLink": "下載 Intel 版",
    "dl.archHintIsIntel": "用的是 Apple Silicon Mac？{link}。",
    "dl.archHintIsIntelLink": "下載 Apple Silicon 版",
    "dl.download": "下載",
    "dl.checksum": "SHA-256",
    "dl.hashCopied": "已複製",
    "dl.source": "下載由 {mirror} 提供。{link}。",
    "dl.sourceLink": "查看更新說明",
    "dl.note.macos-arm64": "適用於 M 系列晶片的 Mac",
    "dl.note.macos-x64": "適用於 Intel 晶片的 Mac",
    "dl.note.windows-x64": "標準的 Windows 安裝程式",
    "dl.note.linux-appimage": "大多數發行版通用，不確定就選這個",
    "dl.note.linux-deb": "適用於 Ubuntu 和 Debian",
    "dl.fallback":
      '下載列表沒能載入。所有安裝檔在 <a href="https://github.com/OnlistTeam/ai-manager/releases">GitHub Releases 頁面</a>上也有一份。',
    "dl.prerelease":
      "這是一個<strong>預先發行版本</strong>。它和正式版用同樣的方式簽章與驗證，但還沒有在每個平台上走完乾淨機器驗證。請預期會有粗糙之處，並且給你在意的東西留一份備份。",

    "shot.alt": "AI 管家視窗，列出 Claude Code、Codex CLI、OpenCode 等工具的版本與更新按鈕。",
    "shot.caption": "電腦上的每一個 AI 編程工具，都在同一個視窗裡：裝了哪些、哪些該更新、每個正連著哪個服務。",
    "verify.heading": "安裝之前",
    "verify.summary": "簽章、首次執行的提示，以及怎麼校驗下載的檔案。",
    "verify.macos.title": "macOS",
    "verify.macos.body":
      "應用程式和磁碟映像檔都以 Apple Developer ID 簽章、經 Apple 公證並已 staple，所以 macOS 開啟它們不會出現警告。如果 Gatekeeper 報錯，代表檔案在發布之後被改動過，不要開啟。",
    "verify.windows.title": "Windows",
    "verify.windows.lead":
      "安裝程式<strong>沒有 Authenticode 簽章</strong>，所以首次執行時 SmartScreen 會顯示「不明發行者」警告。這是預期內的。購買程式碼簽章憑證是一個經過權衡的否決，取而代之的完整性證據是公布的 SHA-256，值得核對一下：",
    "verify.windows.compare":
      "把結果和安裝程式旁邊發布的 <code>.msi.sha256</code> 檔案比對。",
    "verify.linux.title": "Linux",
    "verify.linux.lead":
      "Linux 沒有跨發行版的平台簽章機制，所以這裡同樣由公布的驗證碼來證明下載沒問題：",
    "verify.linux.chmod":
      "AppImage 需要先加上可執行權限（<code>chmod +x</code>）才能執行。",
    "verify.updates.title": "更新",
    "verify.updates.body":
      "裝好之後你就不需要這個頁面了。AI Manager 會在背景檢查更新，用編譯進應用程式的公鑰驗證 minisign 簽章，驗證不通過的一律不下載。安裝仍然由你決定：應用程式會一直等到你按下重新啟動。",

    "footer.source": "原始碼",
    "footer.releases": "全部版本",
    "footer.security": "回報安全漏洞",
    "footer.notices": "第三方聲明",
    "footer.licence":
      'AI Manager 是自由軟體，採用 <a href="https://github.com/OnlistTeam/ai-manager/blob/main/LICENSE">GNU AGPL-3.0-or-later</a> 授權。它所依賴的開源工作的署名在<a href="https://github.com/OnlistTeam/ai-manager/blob/main/THIRD_PARTY_NOTICES.md">第三方聲明</a>裡。',
    "footer.privacy":
      "沒有分析，沒有遙測。這個頁面只載入它自己的檔案，和一份來自我們自己主機的版本清單。",
  },

  ja: {
    "meta.title": "AI Manager をダウンロード",
    "meta.description":
      "AI Manager は、AI コーディング用のコマンドラインツールを自分のコンピューター上でインストール、更新、設定、修復します。無料のオープンソース、AGPL-3.0。",
    "lang.label": "言語",
    "head.tagline": "AI コーディングツールのための静かなデスクトップ管理ツール",
    "head.blurb":
      "Claude Code、Codex、OpenCode とそのほか 7 つのコマンドラインツールを、シェルの設定ファイルや <code>PATH</code> に触れることなく、設定ファイルを手で書き換えることもなくインストール、更新、設定、修復します。データはすべて自分のコンピューターに残ります。",

    "dl.eyebrow": "AI Manager をダウンロード",
    "dl.eyebrowPlatforms": "お使いのシステムを選択",
    "dl.heading": "ダウンロード",
    "dl.loading": "最新リリースを確認しています…",
    "dl.choosePlatform":
      "プラットフォームを選ぶと、インストーラーとチェックサムが表示されます。",
    "dl.platformsLabel": "プラットフォーム",
    "dl.tabMac": "macOS",
    "dl.tabWindows": "Windows",
    "dl.tabLinux": "Linux",
    "dl.allHeading": "すべてのプラットフォーム",
    "dl.versionLine": "バージョン {version}、{date} 公開",
    "dl.releaseNotes": "リリースノート",
    "dl.mobile":
      "AI Manager はデスクトップアプリケーションです。インストールしたい Windows、macOS、Linux のコンピューターでこのページを開いてください。",
    "dl.macArm": "Apple Silicon 版をダウンロード",
    "dl.macIntel": "Intel 版をダウンロード",
    "dl.windows": "Windows 版をダウンロード",
    "dl.linux": "AppImage をダウンロード",
    "dl.macArmName": "Apple Silicon インストーラー（.dmg）",
    "dl.macIntelName": "Intel インストーラー（.dmg）",
    "dl.windowsName": "Windows インストーラー（.msi）",
    "dl.appImageName": "AppImage",
    "dl.debName": "Deb パッケージ",
    "dl.macosCardTitle": "インストーラーイメージ（.dmg）",
    "dl.macosCardBody":
      "ディスクイメージを開き、AI Manager を Applications にドラッグしてください。",
    "dl.windowsCardTitle": "Windows インストーラー（.msi）",
    "dl.windowsCardBody":
      "インストーラーを実行し、画面の指示に従ってください。",
    "dl.linuxCardTitle": "Linux パッケージ",
    "dl.linuxCardBody":
      "ほとんどのディストリビューションでは AppImage、Ubuntu と Debian では deb を選べます。",
    "dl.archHintBoth":
      "どちらか分からないときは、アップルメニュー › このMacについて。チップ名に「Apple M」が含まれていれば Apple Silicon です。",
    "dl.archHintOther": "Intel Mac をお使いですか。{link}。",
    "dl.archHintOtherLink": "Intel 版をダウンロード",
    "dl.archHintIsIntel": "Apple Silicon Mac をお使いですか。{link}。",
    "dl.archHintIsIntelLink": "Apple Silicon 版をダウンロード",
    "dl.download": "ダウンロード",
    "dl.checksum": "SHA-256",
    "dl.hashCopied": "コピーしました",
    "dl.source": "ダウンロードは {mirror} から配信しています。{link}。",
    "dl.sourceLink": "リリースノートを読む",
    "dl.note.macos-arm64": "M シリーズチップの Mac 向け",
    "dl.note.macos-x64": "Intel チップの Mac 向け",
    "dl.note.windows-x64": "標準の Windows インストーラー",
    "dl.note.linux-appimage":
      "ほとんどのディストリビューションで動作します。迷ったらこれを",
    "dl.note.linux-deb": "Ubuntu と Debian 向け",
    "dl.fallback":
      'ダウンロード一覧を読み込めませんでした。すべてのインストーラーは <a href="https://github.com/OnlistTeam/ai-manager/releases">GitHub Releases ページ</a>にもあります。',
    "dl.prerelease":
      "これは<strong>プレリリース</strong>です。署名と検証は正式リリースと同じ方法で行われていますが、すべてのプラットフォームでのクリーンマシン検証はまだ完了していません。粗さがあることを想定し、大事なものはバックアップを取っておいてください。",

    "shot.alt": "AI Manager のウィンドウ。Claude Code、Codex CLI、OpenCode などのバージョンと更新ボタンが並んでいます。",
    "shot.caption": "このコンピュータの AI コーディングツールが一つのウィンドウに。何が入っていて、何を更新すべきで、それぞれどのサービスに繋がっているか。",
    "verify.heading": "インストールの前に",
    "verify.summary": "署名、初回起動時の警告、ダウンロードの検証方法。",
    "verify.macos.title": "macOS",
    "verify.macos.body":
      "アプリとディスクイメージは Apple Developer ID で署名され、Apple の公証を受け、staple 済みです。そのため macOS は警告なしで開きます。Gatekeeper が警告を出した場合、そのファイルは公開後に改変されています。開かないでください。",
    "verify.windows.title": "Windows",
    "verify.windows.lead":
      "インストーラーに <strong>Authenticode 署名はありません</strong>。そのため初回実行時に SmartScreen が「不明な発行元」の警告を表示します。これは想定どおりです。コード署名証明書を購入しないことは意図的な判断であり、その代わりの完全性の根拠が公開された SHA-256 です。確認する価値があります。",
    "verify.windows.compare":
      "結果を、インストーラーと並べて公開されている <code>.msi.sha256</code> ファイルと照合してください。",
    "verify.linux.title": "Linux",
    "verify.linux.lead":
      "Linux にはディストリビューションをまたぐ署名の仕組みがありません。ここでも公開されたチェックサムがダウンロードを証明します。",
    "verify.linux.chmod":
      "AppImage は実行権限（<code>chmod +x</code>）を付けないと起動しません。",
    "verify.updates.title": "更新",
    "verify.updates.body":
      "一度インストールすれば、このページはもう必要ありません。AI Manager はバックグラウンドで更新を確認し、アプリに組み込まれた公開鍵で minisign 署名を検証し、検証できないものは一切ダウンロードしません。インストールするかどうかは明示的な選択のままです。アプリは再起動を押すまで待ちます。",

    "footer.source": "ソースコード",
    "footer.releases": "すべてのリリース",
    "footer.security": "脆弱性を報告",
    "footer.notices": "サードパーティー表記",
    "footer.licence":
      'AI Manager は <a href="https://github.com/OnlistTeam/ai-manager/blob/main/LICENSE">GNU AGPL-3.0-or-later</a> のもとで公開されている自由ソフトウェアです。土台となっているオープンソースへの帰属表示は<a href="https://github.com/OnlistTeam/ai-manager/blob/main/THIRD_PARTY_NOTICES.md">サードパーティー表記</a>にあります。',
    "footer.privacy":
      "アナリティクスもテレメトリーもありません。このページが読み込むのは自身のファイルと、自社ホストにあるリリースマニフェスト 1 つだけです。",
  },
};

/** The English copy already in the document, captured before anything replaces it. */
const ENGLISH = new Map();

function captureEnglish() {
  for (const node of document.querySelectorAll("[data-i18n]")) {
    ENGLISH.set(node.dataset.i18n, node.innerHTML);
  }
  for (const node of document.querySelectorAll("[data-i18n-aria]")) {
    ENGLISH.set(node.dataset.i18nAria, node.getAttribute("aria-label") ?? "");
  }
  // Alternative text is read instead of the image, so it is copy like any
  // other and follows the language the rest of the page is in.
  for (const node of document.querySelectorAll("[data-i18n-alt]")) {
    ENGLISH.set(node.dataset.i18nAlt, node.getAttribute("alt") ?? "");
  }
  ENGLISH.set("meta.title", document.title);
  ENGLISH.set(
    "meta.description",
    document.querySelector('meta[name="description"]')?.content ?? "",
  );
}

function known(value) {
  return LANGUAGES.includes(value) ? value : null;
}

/**
 * An explicit `?lang=` wins so a link can point at one language, then a choice
 * made on an earlier visit, and only then what the browser asks for.
 */
function initialLanguage() {
  const requested = known(new URLSearchParams(location.search).get("lang"));
  if (requested) return requested;

  const remembered = known(localStorage.getItem(STORAGE_KEY));
  if (remembered) return remembered;

  for (const tag of navigator.languages ?? [navigator.language]) {
    const lower = String(tag).toLowerCase();
    // Traditional Chinese has to be tested before Chinese, or Taiwan and Hong
    // Kong visitors get Simplified.
    if (/^zh-(tw|hk|mo)/.test(lower) || lower.includes("hant")) return "zh-TW";
    if (lower.startsWith("zh")) return "zh";
    if (lower.startsWith("ja")) return "ja";
    if (lower.startsWith("en")) return "en";
  }
  return FALLBACK;
}

let current = FALLBACK;
const listeners = new Set();

function translate(key, language) {
  return COPY[language]?.[key] ?? ENGLISH.get(key) ?? "";
}

function apply(language) {
  current = language;
  document.documentElement.lang = language === "zh" ? "zh-Hans" : language;
  document.title = translate("meta.title", language);

  const description = document.querySelector('meta[name="description"]');
  if (description)
    description.content = translate("meta.description", language);

  for (const node of document.querySelectorAll("[data-i18n]")) {
    node.innerHTML = translate(node.dataset.i18n, language);
  }

  for (const node of document.querySelectorAll("[data-i18n-aria]")) {
    node.setAttribute("aria-label", translate(node.dataset.i18nAria, language));
  }

  for (const node of document.querySelectorAll("[data-i18n-alt]")) {
    node.setAttribute("alt", translate(node.dataset.i18nAlt, language));
  }

  for (const button of document.querySelectorAll("[data-lang]")) {
    button.setAttribute(
      "aria-pressed",
      String(button.dataset.lang === language),
    );
  }

  for (const listener of listeners) listener(language);
}

function select(language) {
  if (!known(language) || language === current) return;
  localStorage.setItem(STORAGE_KEY, language);
  apply(language);
}

captureEnglish();

for (const button of document.querySelectorAll("[data-lang]")) {
  button.addEventListener("click", () => select(button.dataset.lang));
}

window.pageI18n = {
  /** The active language tag. */
  language: () => current,
  /** The active language as something `Intl` understands. */
  dateLocale: () => DATE_LOCALES[current],
  /** One string, falling back to the English already in the document. */
  t: (key) => translate(key, current),
  /** Called after every language change, including none at load. */
  onChange(listener) {
    listeners.add(listener);
  },
};

apply(initialLanguage());
