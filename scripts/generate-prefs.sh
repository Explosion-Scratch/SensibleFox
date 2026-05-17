#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"
OUT_DIR="$ROOT_DIR/generated"

mkdir -p "$OUT_DIR"

echo "sensiblefox: generating prefs"
echo "=============================="

fetch() {
    local name="$1"
    local url="$2"
    local outfile="$3"
    printf "  ↓ %-30s " "$name..."
    if curl -fsSL "$url" -o "$OUT_DIR/$outfile" 2>/dev/null; then
        lines=$(wc -l < "$OUT_DIR/$outfile" | tr -d ' ')
        echo "ok ($lines lines)"
    else
        echo "FAILED"
        echo "" > "$OUT_DIR/$outfile"
    fi
}

fetch "Betterfox/Fastfox"   "https://raw.githubusercontent.com/yokoffing/Betterfox/main/Fastfox.js"   "betterfox-fastfox.js"
fetch "Betterfox/Peskyfox"  "https://raw.githubusercontent.com/yokoffing/Betterfox/main/Peskyfox.js"  "betterfox-peskyfox.js"
fetch "Betterfox/Securefox" "https://raw.githubusercontent.com/yokoffing/Betterfox/main/Securefox.js" "betterfox-securefox.js"
fetch "arkenfox/user.js"    "https://raw.githubusercontent.com/arkenfox/user.js/master/user.js"       "arkenfox-user.js"

echo ""
echo "  Assembling single opinionated user.js..."

cat > "$OUT_DIR/user.js" << 'HEADER'
// ╔══════════════════════════════════════════════════════════════════════════╗
// ║  sensiblefox — opinionated Firefox preferences                         ║
// ║  Auto-generated. Regenerate with: ./scripts/generate-prefs.sh          ║
// ║                                                                        ║
// ║  Sources:                                                              ║
// ║    • Betterfox (yokoffing) — Fastfox, Peskyfox, Securefox              ║
// ║    • arkenfox/user.js (reference, not merged directly)                 ║
// ║    • sensiblefox overrides — passwords, DNS, devtools, macOS, etc.     ║
// ╚══════════════════════════════════════════════════════════════════════════╝
HEADER

# ──────────────────────────────────────────────
# SECTION 1: Betterfox upstream
# ──────────────────────────────────────────────
for f in betterfox-fastfox.js betterfox-securefox.js betterfox-peskyfox.js; do
    if [ -s "$OUT_DIR/$f" ]; then
        {
            echo ""
            echo "// ═══════════════════════════════════════════"
            echo "// UPSTREAM: $f"
            echo "// ═══════════════════════════════════════════"
            cat "$OUT_DIR/$f"
        } >> "$OUT_DIR/user.js"
    fi
done

# ──────────────────────────────────────────────
# SECTION 2: sensiblefox overrides
# These take precedence over upstream (last write wins in user.js)
# ──────────────────────────────────────────────

cat >> "$OUT_DIR/user.js" << 'SENSIBLEFOX'


// ╔══════════════════════════════════════════════════════════════════════════╗
// ║  SENSIBLEFOX OVERRIDES — these take precedence over upstream           ║
// ╚══════════════════════════════════════════════════════════════════════════╝

// ═══════════════════════════════════════════
// TELEMETRY — kill it all
// ═══════════════════════════════════════════
user_pref("datareporting.policy.dataSubmissionEnabled", false);
user_pref("datareporting.healthreport.uploadEnabled", false);
user_pref("toolkit.telemetry.unified", false);
user_pref("toolkit.telemetry.enabled", false);
user_pref("toolkit.telemetry.server", "data:,");
user_pref("toolkit.telemetry.archive.enabled", false);
user_pref("toolkit.telemetry.newProfilePing.enabled", false);
user_pref("toolkit.telemetry.shutdownPingSender.enabled", false);
user_pref("toolkit.telemetry.updatePing.enabled", false);
user_pref("toolkit.telemetry.bhrPing.enabled", false);
user_pref("toolkit.telemetry.firstShutdownPing.enabled", false);
user_pref("toolkit.telemetry.coverage.opt-out", true);
user_pref("toolkit.coverage.opt-out", true);
user_pref("toolkit.coverage.endpoint.base", "");
user_pref("browser.newtabpage.activity-stream.feeds.telemetry", false);
user_pref("browser.newtabpage.activity-stream.telemetry", false);
user_pref("browser.search.serpEventTelemetry.enabled", false);
user_pref("corroborator.enabled", false);
user_pref("media.webvtt.debug.logging", false);
user_pref("media.webvtt.testing.events", false);
user_pref("browser.contentblocking.database.enabled", false);
user_pref("browser.contentblocking.cfr-milestone.enabled", false);
user_pref("default-browser-agent.enabled", false);
user_pref("dom.private-attribution.submission.enabled", false);
user_pref("datareporting.policy.dataSubmissionPolicyAcceptedVersion", 2);
user_pref("toolkit.telemetry.prompted", 2);

// ═══════════════════════════════════════════
// FIREFOX ACCOUNT & SYNC — disabled
// ═══════════════════════════════════════════
user_pref("identity.fxaccounts.enabled", false);
user_pref("services.sync.enabled", false);
user_pref("webextensions.storage.sync.enabled", false);

// ═══════════════════════════════════════════
// STUDIES & EXPERIMENTS — disabled
// ═══════════════════════════════════════════
user_pref("app.shield.optoutstudies.enabled", false);
user_pref("app.normandy.enabled", false);
user_pref("app.normandy.api_url", "");

// ═══════════════════════════════════════════
// CRASH REPORTS — disabled
// ═══════════════════════════════════════════
user_pref("breakpad.reportURL", "");
user_pref("browser.tabs.crashReporting.sendReport", false);
user_pref("browser.crashReports.unsubmittedCheck.autoSubmit2", false);
user_pref("browser.crashReports.unsubmittedCheck.enabled", false);

// ═══════════════════════════════════════════
// PASSWORD MANAGER — disabled (use a real one)
// ═══════════════════════════════════════════
user_pref("signon.rememberSignons", false);
user_pref("signon.generation.enabled", false);
user_pref("signon.management.page.breach-alerts.enabled", false);
user_pref("signon.management.page.breachAlertUrl", "");
user_pref("security.ask_for_password", 2);
user_pref("security.password_lifetime", 5);
user_pref("signon.autofillForms", false);
user_pref("signon.formlessCapture.enabled", false);
user_pref("network.auth.subresource-http-auth-allow", 1);
user_pref("extensions.formautofill.addresses.enabled", false);
user_pref("extensions.formautofill.creditCards.enabled", false);

// ═══════════════════════════════════════════
// SPONSORED CONTENT & MOZILLA PROMOTIONS — removed
// ═══════════════════════════════════════════
user_pref("browser.newtabpage.activity-stream.showSponsored", false);
user_pref("browser.newtabpage.activity-stream.showSponsoredTopSites", false);
user_pref("browser.newtabpage.activity-stream.default.sites", "");
user_pref("browser.urlbar.suggest.quicksuggest.sponsored", false);
user_pref("browser.urlbar.suggest.quicksuggest.nonsponsored", false);
user_pref("browser.urlbar.suggest.quicksuggest.all", false);
user_pref("browser.urlbar.quicksuggest.enabled", false);
user_pref("browser.urlbar.sponsoredTopSites", false);
user_pref("browser.privatebrowsing.vpnpromourl", "");
user_pref("browser.vpn_promo.enabled", false);
user_pref("extensions.getAddons.showPane", false);
user_pref("extensions.htmlaboutaddons.recommendations.enabled", false);
user_pref("extensions.autoDisableScopes", 0);
user_pref("browser.discovery.enabled", false);
user_pref("browser.shopping.experience2023.enabled", false);
user_pref("browser.shopping.experience2023.ads.exposure", false);
user_pref("browser.shell.checkDefaultBrowser", false);
user_pref("browser.newtabpage.activity-stream.asrouter.userprefs.cfr.addons", false);
user_pref("browser.newtabpage.activity-stream.asrouter.userprefs.cfr.features", false);
user_pref("browser.preferences.moreFromMozilla", false);
user_pref("browser.messaging-system.whatsNewPanel.enabled", false);

// ═══════════════════════════════════════════
// WELCOME & FIRST-RUN — skip everything
// ═══════════════════════════════════════════
user_pref("browser.aboutConfig.showWarning", false);
user_pref("browser.aboutwelcome.enabled", false);
user_pref("startup.homepage_welcome_url", "");
user_pref("startup.homepage_welcome_url.additional", "");
user_pref("startup.homepage_override_url", "");
user_pref("trailhead.firstrun.didSeeAboutWelcome", true);
user_pref("browser.startup.homepage_override.mstone", "ignore");
user_pref("trailhead.firstrun.branches", "nofirstrun-empty");
user_pref("browser.startup.page", 1);
user_pref("browser.startup.couldRestoreSession.count", -1);

// ═══════════════════════════════════════════
// POCKET — disabled
// ═══════════════════════════════════════════
user_pref("extensions.pocket.enabled", false);
user_pref("extensions.pocket.api", " ");
user_pref("extensions.pocket.oAuthConsumerKey", " ");
user_pref("extensions.pocket.site", " ");
user_pref("extensions.pocket.showHome", false);

// ═══════════════════════════════════════════
// DNS OVER HTTPS — Quad9
// ═══════════════════════════════════════════
user_pref("network.trr.custom_uri", "https://dns.quad9.net/dns-query");
user_pref("network.trr.mode", 2);
user_pref("network.trr.uri", "https://dns.quad9.net/dns-query");

// ═══════════════════════════════════════════
// DEVTOOLS — docked right, ready to use
// ═══════════════════════════════════════════
user_pref("devtools.toolbox.host", "right");
user_pref("devtools.debugger.remote-enabled", true);
user_pref("devtools.everOpened", true);
user_pref("devtools.selfxss.count", 5);
user_pref("devtools.toolbox.selectedTool", "webconsole");

// ═══════════════════════════════════════════
// MACOS APPEARANCE — native blur, smooth fonts
// ═══════════════════════════════════════════
user_pref("widget.macos.titlebar-blend-mode.behind-window", true);
user_pref("browser.theme.macos.native-theme", true);
user_pref("browser.theme.native-theme", true);
user_pref("gfx.use_text_smoothing_setting", true);
user_pref("browser.theme.dark-private-windows", false);
user_pref("browser.privateWindowSeparation.enabled", false);
user_pref("layout.css.prefers-color-scheme.content-override", 2);
user_pref("layout.css.backdrop-filter.enabled", true);
user_pref("gfx.font_rendering.cleartype_params.rendering_mode", 5);
user_pref("gfx.font_rendering.cleartype_params.enhanced_contrast", 0);
user_pref("cg.font_rendering.font_smoothing_disabled", true);


// ═══════════════════════════════════════════
// PERFORMANCE — GPU acceleration, modern codecs
// ═══════════════════════════════════════════
user_pref("layers.acceleration.force-enabled", true);
user_pref("gfx.webrender.all", true);
user_pref("gfx.webrender.quality.force-subpixel-aa-where-possible", true);
user_pref("gfx.webrender.precache-shaders", true);
user_pref("gfx.webrender.compositor.force-enabled", true);
user_pref("gfx.webrender.layer-compositor", true);
user_pref("gfx.canvas.accelerated.cache-items", 4096);
user_pref("gfx.canvas.accelerated.cache-size", 512);
user_pref("webgl.max-size", 16384);
user_pref("webgl.force-enabled", true);
user_pref("layers.gpu-process.force-enabled", true);
user_pref("media.hardware-video-decoding.force-enabled", true);
user_pref("image.jxl.enabled", true);

// ═══════════════════════════════════════════
// FASTFOX — curated speed tweaks (Betterfox/Fastfox)
// ═══════════════════════════════════════════
user_pref("nglayout.initialpaint.delay", 0);
user_pref("nglayout.initialpaint.delay_in_oopif", 0);
user_pref("content.notify.interval", 100000);
user_pref("browser.cache.jsbc_compression_level", 3);
user_pref("browser.cache.disk.metadata_memory_limit", 16384);
user_pref("browser.cache.memory.max_entry_size", 20480);
user_pref("media.memory_caches_combined_limit_kb", 1048576);
user_pref("media.cache_readahead_limit", 600);
user_pref("media.cache_resume_threshold", 300);
user_pref("image.cache.size", 10485760);
user_pref("image.mem.decode_bytes_at_a_time", 65536);
user_pref("network.buffer.cache.size", 65535);
user_pref("network.buffer.cache.count", 48);
user_pref("network.http.max-connections", 1800);
user_pref("network.http.max-persistent-connections-per-server", 30);
user_pref("network.http.max-urgent-start-excessive-connections-per-host", 10);
user_pref("network.http.pacing.requests.enabled", false);
user_pref("network.dnsCacheEntries", 10000);
user_pref("network.dnsCacheExpiration", 3600);
user_pref("network.dnsCacheExpirationGracePeriod", 120);
user_pref("network.dns.disableIPv6", false);
user_pref("network.ssl_tokens_cache_capacity", 10240);
user_pref("browser.tabs.min_inactive_duration_before_unload", 300000);
user_pref("dom.ipc.processPrelaunch.fission.number", 1);

// Added from Betterfox/Fastfox master
user_pref("dom.enable_web_task_scheduling", true);
user_pref("dom.script_loader.bytecode_cache.enabled", true);
user_pref("dom.script_loader.bytecode_cache.strategy", 0);
user_pref("network.http.request.max-start-delay", 5);
user_pref("network.websocket.max-connections", 200);
user_pref("network.dns.max_high_priority_threads", 40);
user_pref("network.dns.max_any_priority_threads", 24);
user_pref("browser.cache.disk.max_chunks_memory_usage", 40960);
user_pref("browser.cache.disk.max_priority_chunks_memory_usage", 40960);
user_pref("browser.cache.disk.preload_chunk_count", 4);
user_pref("browser.cache.frecency_half_life_hours", 6);
user_pref("browser.cache.check_doc_frequency", 3);
user_pref("image.mem.shared.unmap.min_expiration_ms", 120000);
user_pref("dom.iframe_lazy_loading.enabled", true);

// ═══════════════════════════════════════════
// SPEED POLICY — tunable goals (override locally if you want a different tradeoff)
// ═══════════════════════════════════════════
// Keep history forever:
//   • places.history.expiration.max_pages → high cap (below); alternatively lower for less SQLite work
// Prefer RAM over disk:
//   • Large browser.cache.memory.capacity; keep disk cache on for revisits, or set
//     browser.cache.disk.enable false to minimize disk (hurts cold/offline repeat loads)
// Prefer higher performance over lower RAM:
//   • Raise memory/disk caches, browser.sessionhistory.max_total_viewers, dom.ipc.processCount*
//   • Lower those toward defaults if you need to shrink footprint
// Accessibility overhead off (breaks a11y features):
//   • accessibility.force_disabled → 1
// Reader: skip readability scan on every pageload:
//   • reader.parse-on-load.enabled → false (reader icon/heuristics less aggressive)
// Higher parallelism (more isolated web processes → more RAM):
//   • dom.ipc.processCount / dom.ipc.processCount.webIsolated (below)
// Session snapshots less often (less disk IO; rougher crash recovery window):
//   • browser.sessionstore.interval (ms; below uses 60s — default often 15s)
// GPU path where available:
//   • gfx.canvas.accelerated + existing WebRender/GPU prefs above
// Speculative networking (more background DNS/connects; snappier perceived loads):
//   • network.predictor*, prefetch-next, dns prefetch on, urlbar speculative connect
//
user_pref("places.history.expiration.max_pages", 2147483647);
user_pref("browser.cache.memory.capacity", 262144);
user_pref("browser.cache.disk.smart_size.enabled", false);
user_pref("browser.cache.disk.capacity", 1048576);
user_pref("browser.sessionhistory.max_total_viewers", 16);
user_pref("dom.ipc.processCount", 8);
user_pref("dom.ipc.processCount.webIsolated", 8);
user_pref("browser.sessionstore.interval", 60000);
user_pref("accessibility.force_disabled", 1);
user_pref("reader.parse-on-load.enabled", false);
user_pref("gfx.canvas.accelerated", true);
user_pref("network.predictor.enabled", true);
user_pref("network.predictor.enable-prefetch", true);
user_pref("network.predictor.enable-hover-on-ssl", true);
user_pref("network.prefetch-next", true);
user_pref("network.dns.disablePrefetch", false);
user_pref("network.dns.disablePrefetchFromHTTPS", false);
user_pref("network.preconnect", true);
user_pref("network.http.speculative-parallel-limit", 18);
user_pref("browser.urlbar.speculativeConnect.enabled", true);
user_pref("browser.places.speculativeConnect.enabled", true);
user_pref("network.early-hints.enabled", true);
user_pref("network.early-hints.preconnect.enabled", true);
user_pref("network.http.priority_header.enabled", true);
user_pref("network.http.http2.send-priority-frames", true);
user_pref("network.http.http3.enable", true);
user_pref("network.http.http3.support_version1", true);
user_pref("network.http.http3.enable_0rtt", true);
user_pref("browser.startup.preXulSkeletonUI", true);
user_pref("browser.privatebrowsing.resetPBM.showConfirmationDialog", false);

// ═══════════════════════════════════════════
// PRIVACY — strict tracking protection
// ═══════════════════════════════════════════
user_pref("browser.contentblocking.category", "strict");
user_pref("privacy.trackingprotection.enabled", true);
user_pref("privacy.trackingprotection.socialtracking.enabled", true);
user_pref("privacy.trackingprotection.cryptomining.enabled", true);
user_pref("privacy.trackingprotection.fingerprinting.enabled", true);
user_pref("network.cookie.cookieBehavior", 5);
user_pref("privacy.sanitize.clearOnShutdown.hasMigratedToNewPrefs2", true);

// ═══════════════════════════════════════════
// COOKIE BANNERS — auto-reject
// ═══════════════════════════════════════════
user_pref("cookiebanners.service.mode", 2);
user_pref("cookiebanners.service.mode.privateBrowsing", 2);

// ═══════════════════════════════════════════
// FULLSCREEN — no delay, no warning
// ═══════════════════════════════════════════
user_pref("full-screen-api.transition-duration.enter", "0 0");
user_pref("full-screen-api.transition-duration.leave", "0 0");
user_pref("full-screen-api.warning.delay", -1);
user_pref("full-screen-api.warning.timeout", 0);
user_pref("browser.fullscreen.autohide", true);

// ═══════════════════════════════════════════
// URL BAR & SEARCH SUGGESTIONS — default engine (Google) + history,
// bookmarks, open tabs; calculator & unit conversion on; no extra engines, top sites
// ═══════════════════════════════════════════
user_pref("browser.search.suggest.enabled", true);
user_pref("browser.urlbar.suggest.searches", true);
user_pref("browser.urlbar.suggest.history", true);
user_pref("browser.urlbar.suggest.bookmark", true);
user_pref("browser.urlbar.suggest.openpage", true);
user_pref("browser.urlbar.suggest.engines", false);
user_pref("browser.urlbar.suggest.topsites", false);
user_pref("browser.urlbar.suggest.calculator", true);
user_pref("browser.urlbar.unitConversion.enabled", true);
user_pref("browser.urlbar.trending.featureGate", false);
user_pref("browser.search.hiddenOneOffs", "Amazon.com,eBay,Perplexity");
user_pref("browser.urlbar.scotchBonnet.enableOverride", false);
user_pref("browser.tabs.tabmanager.enabled", false);

// ═══════════════════════════════════════════
// CONTEXT MENU CLEANUP — remove bloat
// ═══════════════════════════════════════════
user_pref("browser.translations.select.enable", false);
user_pref("dom.text_fragments.enabled", false);
user_pref("privacy.query_stripping.strip_on_share.enabled", false);
user_pref("devtools.accessibility.enabled", false);
user_pref("browser.ml.chat.menu", false);
user_pref("browser.ml.linkPreview.enabled", false);
user_pref("dom.text-recognition.enabled", false);
user_pref("browser.search.visualSearch.featureGate", false);
user_pref("widget.macos.native-context-menus", false);
user_pref("browser.search.separatePrivateDefault.ui.enabled", false);

// ═══════════════════════════════════════════
// TAB & UX BEHAVIOR — sensible defaults
// ═══════════════════════════════════════════
user_pref("browser.link.open_newwindow", 3);
user_pref("browser.link.open_newwindow.restriction", 0);
user_pref("browser.tabs.splitView.enabled", false);
user_pref("browser.tabs.warnOnClose", false);
user_pref("browser.warnOnQuitShortcut", false);
user_pref("browser.warnOnQuit", false);
user_pref("browser.tabs.hoverPreview.enabled", true);
user_pref("browser.tabs.hoverPreview.showThumbnails", true);
user_pref("browser.bookmarks.openInTabClosesMenu", false);
user_pref("browser.menu.showViewImageInfo", false);
user_pref("findbar.highlightAll", true);
user_pref("layout.word_select.eat_space_to_next_word", false);
user_pref("editor.word_select.delete_space_after_doubleclick_selection", true);
user_pref("dom.disable_window_move_resize", true);
user_pref("media.videocontrols.picture-in-picture.video-toggle.enabled", false);
user_pref("accessibility.typeaheadfind", false);
user_pref("media.autoplay.blocking_policy", 2);
user_pref("screenshots.browser.component.enabled", false);
user_pref("browser.search.context.loadInBackground", true);

// ═══════════════════════════════════════════
// DOWNLOADS — sensible handling
// ═══════════════════════════════════════════
user_pref("browser.download.always_ask_before_handling_new_types", false);
user_pref("browser.download.manager.addToRecentDocs", false);
user_pref("browser.download.autohideButton", true);
user_pref("browser.download.alwaysOpenPanel", false);
user_pref("browser.download.manager.showWhenStarting", false);
user_pref("browser.download.open_pdf_attachments_inline", true);
user_pref("browser.download.useDownloadDir", true);

// ═══════════════════════════════════════════
// FORM HISTORY — disabled (replaces DisableFormHistory policy)
// ═══════════════════════════════════════════
user_pref("browser.formfill.enable", false);

// ═══════════════════════════════════════════
// MISC UX — polish
// ═══════════════════════════════════════════
user_pref("widget.non-native-theme.scrollbar.style", 1);
user_pref("ui.useOverlayScrollbars", 1);
user_pref("browser.compactmode.show", true);
user_pref("browser.display.focus_ring_on_anything", true);
user_pref("browser.display.focus_ring_style", 0);
user_pref("browser.display.focus_ring_width", 0);
user_pref("layout.spellcheckDefault", 2);
user_pref("ui.SpellCheckerUnderlineStyle", 1);
user_pref("browser.bookmarks.max_backups", 5);
user_pref("pdfjs.sidebarViewOnLoad", 1);
user_pref("browser.helperApps.showOpenOptionForPdfJS", true);
user_pref("browser.toolbars.bookmarks.visibility", "newtab");
user_pref("browser.newtabpage.activity-stream.discoverystream.enabled", false);
user_pref("browser.newtabpage.activity-stream.showSearch", true);
user_pref("browser.newtabpage.activity-stream.feeds.topsites", false);
user_pref("browser.newtabpage.activity-stream.improvesearch.topSiteSearchShortcuts", false);
user_pref("browser.newtabpage.activity-stream.feeds.section.topstories", false);
user_pref("browser.newtabpage.activity-stream.feeds.section.highlights", false);
user_pref("browser.newtabpage.activity-stream.feeds.snippets", false);

// ═══════════════════════════════════════════
// EXTENSION SIGNATURES — developer-channel compatibility
// Stable Firefox may ignore these; SensibleFox's bundled add-ons are signed.
// ═══════════════════════════════════════════
user_pref("xpinstall.signatures.required", false);
user_pref("xpinstall.whitelist.required", false);
user_pref("extensions.langpacks.signatures.required", false);
user_pref("extensions.experiments.enabled", true);
user_pref("extensions.install.requireBuiltInCerts", false);
user_pref("extensions.update.requireBuiltInCerts", false);

// ═══════════════════════════════════════════
// REFERRER POLICY — strict-origin-when-cross-origin (Firefox default)
// • cross-origin: scheme+host+port only (no path/query)
// • same-origin:  full referrer
// • never spoof — sites use it for legitimate auth flows
// ═══════════════════════════════════════════
user_pref("network.http.sendRefererHeader", 2);
user_pref("network.http.referer.spoofSource", false);
user_pref("network.http.referer.XOriginPolicy", 0);
user_pref("network.http.referer.XOriginTrimmingPolicy", 2);
user_pref("network.http.referer.trimmingPolicy", 0);
user_pref("network.http.referer.defaultPolicy", 2);
user_pref("network.http.referer.defaultPolicy.pbmode", 2);
user_pref("network.http.referer.disallowCrossSiteRelaxingDefault", true);
user_pref("network.http.referer.hideOnionSource", true);

// ═══════════════════════════════════════════
// TRACKING — modern signals, sensible defaults
// ═══════════════════════════════════════════
user_pref("privacy.globalprivacycontrol.enabled", true);
user_pref("privacy.globalprivacycontrol.functionality.enabled", true);
user_pref("privacy.globalprivacycontrol.pbmode.enabled", true);
user_pref("privacy.donottrackheader.enabled", true);
user_pref("privacy.fingerprintingProtection", true);
user_pref("privacy.query_stripping.enabled", true);
user_pref("privacy.query_stripping.enabled.pbmode", true);

// ═══════════════════════════════════════════
// WARNING BYPASS — keep safety nets, but never block the user
// detection stays ON; the user can always click through.
// ═══════════════════════════════════════════
user_pref("browser.safebrowsing.allowOverride", true);
user_pref("browser.safebrowsing.malware.enabled", true);
user_pref("browser.safebrowsing.phishing.enabled", true);
user_pref("browser.safebrowsing.blockedURIs.enabled", true);
user_pref("browser.safebrowsing.downloads.enabled", true);
user_pref("security.warn_entering_weak", false);
user_pref("security.warn_entering_weak.show_once", false);
user_pref("security.warn_leaving_secure", false);
user_pref("security.warn_leaving_secure.show_once", false);
user_pref("security.warn_submit_insecure", false);
user_pref("security.warn_submit_secure_to_insecure", false);
user_pref("security.warn_viewing_mixed", false);
user_pref("security.warn_viewing_mixed.show_once", false);
user_pref("security.mixed_content.block_active_content", false);
user_pref("security.mixed_content.block_display_content", false);
user_pref("security.insecure_connection_text.enabled", false);
user_pref("security.insecure_connection_text.pbmode.enabled", false);
user_pref("network.protocol-handler.warn-external-default", false);
user_pref("browser.tabs.warnOnCloseOtherTabs", false);
user_pref("browser.tabs.warnOnOpen", false);
user_pref("browser.sessionstore.warnOnQuit", false);
user_pref("general.warnOnAboutConfig", false);
user_pref("browser.aboutConfig.showWarning", false);

// ═══════════════════════════════════════════
// CUSTOMIZATION ENABLEMENT — userChrome, SVG, toolbar layout
// ═══════════════════════════════════════════
user_pref("toolkit.legacyUserProfileCustomizations.stylesheets", true);
user_pref("browser.legacyUserProfileCustomizations.stylesheets", true);
user_pref("svg.context-properties.content.enabled", true);
SENSIBLEFOX

echo ""
echo "  Deduplicating user_pref keys (keeping last write per key)..."
pre_count=$(grep -c '^[[:space:]]*user_pref(' "$OUT_DIR/user.js" || true)
/usr/bin/perl -e '
  my @lines = <>;
  my %last;
  for my $i (0..$#lines) {
    if ($lines[$i] =~ /^\s*user_pref\(\s*"([^"]+)"/) {
      $last{$1} = $i;
    }
  }
  for my $i (0..$#lines) {
    if ($lines[$i] =~ /^\s*user_pref\(\s*"([^"]+)"/) {
      print $lines[$i] if $last{$1} == $i;
    } else {
      print $lines[$i];
    }
  }
' "$OUT_DIR/user.js" > "$OUT_DIR/user.js.dedup" && mv "$OUT_DIR/user.js.dedup" "$OUT_DIR/user.js"
post_count=$(grep -c '^[[:space:]]*user_pref(' "$OUT_DIR/user.js" || true)
echo "    Removed $((pre_count - post_count)) duplicate user_pref lines"

lines=$(wc -l < "$OUT_DIR/user.js" | tr -d ' ')
prefs=$(grep -c 'user_pref(' "$OUT_DIR/user.js" || true)
echo ""
echo "  ✓ Wrote generated/user.js ($lines lines, $prefs prefs)"
echo "    → Betterfox upstream + sensiblefox overrides"

# ──────────────────────────────────────────────
# Generate sensiblefox-defaults.js (defaultPref form)
# Used by autoconfig (sensiblefox.cfg) in the DMG build
# ──────────────────────────────────────────────
echo ""
echo "  Generating sensiblefox-defaults.js (defaultPref() form)..."

{
    cat <<'DEFAULTS_HEADER'
// sensiblefox-defaults.js — defaultPref() form of user.js
// Concatenated into sensiblefox.cfg by scripts/build-dmg.sh.
// Generated by scripts/generate-prefs.sh — do not edit.
DEFAULTS_HEADER

    /usr/bin/sed 's/^user_pref(/defaultPref(/' "$OUT_DIR/user.js"

    cat <<'TOOLBAR_PREF'

// ═══════════════════════════════════════════
// TOOLBAR LAYOUT — sensiblefox default toolbar customization
// defaultPref() only sets the initial default for new profiles.
// Once the user customizes their toolbar, Firefox writes a user_pref()
// to prefs.js which always takes precedence — user edits are preserved.
// ═══════════════════════════════════════════
defaultPref("browser.uiCustomization.state", "{\"placements\":{\"widget-overflow-fixed-list\":[],\"unified-extensions-area\":[],\"nav-bar\":[\"back-button\",\"forward-button\",\"stop-reload-button\",\"customizableui-special-spring1\",\"urlbar-container\",\"customizableui-special-spring2\",\"downloads-button\",\"unified-extensions-button\",\"ublock0_raymondhill_net-browser-action\"],\"TabsToolbar\":[\"tabbrowser-tabs\",\"new-tab-button\",\"alltabs-button\"],\"PersonalToolbar\":[\"personal-bookmarks\"]},\"seen\":[\"save-to-pocket-button\",\"developer-button\",\"ublock0_raymondhill_net-browser-action\"],\"dirtyAreaCache\":[\"nav-bar\",\"PersonalToolbar\",\"TabsToolbar\"],\"currentVersion\":20,\"newElementCount\":2}");
TOOLBAR_PREF
} > "$OUT_DIR/sensiblefox-defaults.js"

defaults_lines=$(wc -l < "$OUT_DIR/sensiblefox-defaults.js" | tr -d ' ')
defaults_prefs=$(grep -c 'defaultPref(' "$OUT_DIR/sensiblefox-defaults.js" || true)
echo "  ✓ Wrote generated/sensiblefox-defaults.js ($defaults_lines lines, $defaults_prefs prefs)"

echo ""
echo "Done. Next:"
echo "  ./scripts/build.sh             # build everything (PKGs + DMG)"
echo "  ./scripts/build-pkg.sh         # build macOS .pkg installers only"
echo "  ./scripts/build-dmg.sh         # build standalone patched Firefox .dmg only"
echo "  cargo build --release          # build the developer CLI"
