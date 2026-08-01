// Book chrome, progressive navigation, and code sandboxes.

(function () {
  "use strict";

  var root = document.documentElement;
  var script = document.currentScript;
  var scriptUrl = script && script.src;
  var pageCache = new Map();
  var prefetchQueue = [];
  var queuedPages = new Set();
  var activePrefetches = 0;
  var prefetchScheduled = false;
  var navigationVersion = 0;
  var outlineScroll = null;
  var runtime = null;
  var menuButton = document.querySelector(".menu-toggle");

  function pageKey(url) {
    var parsed = new URL(url, window.location.href);
    parsed.hash = "";
    return parsed.href;
  }

  function resolveUrls(scope, base) {
    ["href", "src"].forEach(function (attribute) {
      Array.prototype.forEach.call(scope.querySelectorAll("[" + attribute + "]"), function (node) {
        var value = node.getAttribute(attribute);
        if (!value || /^(data:|javascript:)/i.test(value)) return;
        node.setAttribute(attribute, new URL(value, base).href);
      });
    });
  }

  // The top bar stays mounted between pages, so make its relative links permanent.
  var topbar = document.querySelector(".topbar");
  if (topbar) resolveUrls(topbar, window.location.href);

  pageCache.set(
    pageKey(window.location.href),
    Promise.resolve("<!doctype html>\n" + document.documentElement.outerHTML)
  );

  // --- Theme ---
  var themeButton = document.querySelector(".theme-toggle");
  if (themeButton) {
    themeButton.addEventListener("click", function () {
      var next = root.dataset.theme === "light" ? "navy" : "light";
      root.dataset.theme = next;
      try {
        localStorage.setItem("fsbook-theme", next);
      } catch (e) {
        /* Private mode can discard this preference. */
      }
    });
  }

  // --- Sidebar ---
  if (menuButton) {
    menuButton.addEventListener("click", function () {
      var open = document.body.classList.toggle("sidebar-open");
      menuButton.setAttribute("aria-expanded", String(open));
    });
  }

  function initSidebar() {
    var active = document.querySelector('.chapter-list a[aria-current="page"]');
    var sidebar = document.getElementById("sidebar");
    if (active && sidebar && active.offsetTop > sidebar.clientHeight - 80) {
      sidebar.scrollTop = active.offsetTop - sidebar.clientHeight / 2;
    }
  }

  // --- Outline ---
  function initOutline() {
    if (outlineScroll) {
      window.removeEventListener("scroll", outlineScroll);
      outlineScroll = null;
    }

    var links = Array.prototype.slice.call(document.querySelectorAll(".outline a"));
    if (!links.length) return;

    var targets = links
      .map(function (link) {
        return { link: link, node: document.getElementById(link.hash.slice(1)) };
      })
      .filter(function (entry) {
        return entry.node;
      });
    var current = null;
    var pending = false;

    function mark() {
      var line = window.scrollY + 120;
      var found = targets[0];
      targets.forEach(function (entry) {
        if (entry.node.offsetTop <= line) found = entry;
      });
      if (!found || found.link === current) return;
      if (current) current.classList.remove("is-active");
      found.link.classList.add("is-active");
      current = found.link;
    }

    outlineScroll = function () {
      if (pending) return;
      pending = true;
      window.requestAnimationFrame(function () {
        pending = false;
        mark();
      });
    };
    window.addEventListener("scroll", outlineScroll, { passive: true });
    mark();
  }

  // --- Sandboxes ---
  function loadSandbox() {
    if (runtime) return runtime;
    var base = scriptUrl;
    if (!base) {
      var scripts = document.getElementsByTagName("script");
      base = scripts[scripts.length - 1].src;
    }
    var url = new URL("../sandbox/fsbook_wasm.js", base);
    runtime = import(url.href).then(function (module) {
      return module.default().then(function () {
        return module;
      });
    });
    return runtime;
  }

  function sourceText(node) {
    return node.innerText.replace(/\r\n?/g, "\n").replace(/ /g, " ");
  }

  function showResult(output, result) {
    output.hidden = false;
    output.textContent = "";

    if (result.error) {
      var error = document.createElement("div");
      error.className = "err";
      error.textContent = result.error;
      output.appendChild(error);
      return;
    }

    if (result.uart) {
      var uart = document.createElement("div");
      uart.textContent = result.uart;
      output.appendChild(uart);
    }

    var meta = document.createElement("div");
    meta.className = "meta";
    var parts = [];
    if (result.registers) {
      var interesting = result.registers.filter(function (entry) {
        return entry.value !== "0x0";
      });
      if (interesting.length) {
        parts.push(
          interesting
            .map(function (entry) {
              return entry.name + "=" + entry.value;
            })
            .join("  ")
        );
      }
    }
    parts.push(result.steps + " steps");
    if (result.exit_code !== null && result.exit_code !== undefined) {
      parts.push("exit " + result.exit_code);
    }
    if (result.max_steps_reached) parts.push("step limit reached");
    meta.textContent = parts.join("  |  ");
    output.appendChild(meta);
  }

  function initSandboxes() {
    var sandboxes = Array.prototype.slice.call(document.querySelectorAll(".sandbox"));
    sandboxes.forEach(function (sandbox) {
      var code = sandbox.querySelector(".sandbox-code");
      var original = sandbox.querySelector(".sandbox-source");
      var output = sandbox.querySelector(".sandbox-output");
      var runButton = sandbox.querySelector(".sandbox-run");
      var resetButton = sandbox.querySelector(".sandbox-reset");
      var pristine = code.innerHTML;

      function repaint(module, source) {
        try {
          code.innerHTML = module.highlight_html(sandbox.dataset.lang || "asm", source);
        } catch (e) {
          /* Keep the text as typed if highlighting fails. */
        }
      }

      runButton.addEventListener("click", function () {
        runButton.disabled = true;
        output.hidden = false;
        output.textContent = "Running...";

        loadSandbox()
          .then(function (module) {
            var source = sourceText(code);
            var bare = sandbox.dataset.wrap === "bare";
            var result = module.run_asm(source, 2000000, bare);
            showResult(output, result);
            repaint(module, source);
          })
          .catch(function (error) {
            output.innerHTML = "";
            var node = document.createElement("div");
            node.className = "err";
            node.textContent = "Sandbox failed to load: " + error;
            output.appendChild(node);
          })
          .finally(function () {
            runButton.disabled = false;
          });
      });

      resetButton.addEventListener("click", function () {
        code.innerHTML = pristine;
        output.hidden = true;
        output.textContent = "";
      });

      code.addEventListener("blur", function () {
        if (!runtime) return;
        runtime.then(function (module) {
          repaint(module, sourceText(code));
        });
      });

      if (original) original.value = original.value || "";
    });
  }

  function initPage() {
    initSidebar();
    initOutline();
    initSandboxes();
  }

  // --- Navigation cache ---
  function fetchPage(url) {
    var key = pageKey(url);
    if (pageCache.has(key)) return pageCache.get(key);

    var request = fetch(key, { credentials: "same-origin" })
      .then(function (response) {
        if (!response.ok) throw new Error("Page request failed with " + response.status);
        return response.text();
      })
      .catch(function (error) {
        pageCache.delete(key);
        throw error;
      });
    pageCache.set(key, request);
    return request;
  }

  function queuePage(url) {
    var key = pageKey(url);
    if (pageCache.has(key) || queuedPages.has(key)) return;
    queuedPages.add(key);
    prefetchQueue.push(key);
    schedulePrefetch();
  }

  function runPrefetch() {
    prefetchScheduled = false;
    while (activePrefetches < 3 && prefetchQueue.length) {
      var url = prefetchQueue.shift();
      queuedPages.delete(url);
      activePrefetches += 1;
      fetchPage(url)
        .catch(function () {})
        .finally(function () {
          activePrefetches -= 1;
          if (prefetchQueue.length) schedulePrefetch();
        });
    }
  }

  function schedulePrefetch() {
    if (prefetchScheduled || activePrefetches >= 3) return;
    prefetchScheduled = true;
    if ("requestIdleCallback" in window) {
      window.requestIdleCallback(runPrefetch, { timeout: 1200 });
    } else {
      window.setTimeout(runPrefetch, 80);
    }
  }

  function queueNearbyPages() {
    var links = Array.prototype.slice.call(document.querySelectorAll(".chapter-list a"));
    var current = pageKey(window.location.href);
    var currentIndex = links.findIndex(function (link) {
      return pageKey(link.href) === current;
    });

    links.forEach(function (link, index) {
      if (links.length <= 40 || (currentIndex >= 0 && Math.abs(index - currentIndex) <= 3)) {
        queuePage(link.href);
      }
    });
  }

  function prefetchAdjacentPages() {
    if (!/^https?:$/.test(window.location.protocol)) return;
    Array.prototype.forEach.call(document.querySelectorAll(".pagenav a"), function (link) {
      fetchPage(link.href).catch(function () {});
    });
    queueNearbyPages();
  }

  function isBookLink(link) {
    if (!link || !link.matches(".chapter-list a, .pagenav a, a.brand")) return false;
    if (link.hasAttribute("download") || (link.target && link.target !== "_self")) return false;
    var url = new URL(link.href, window.location.href);
    return url.origin === window.location.origin && /^https?:$/.test(url.protocol);
  }

  function setScroll(url, savedScroll) {
    var previous = root.style.scrollBehavior;
    root.style.scrollBehavior = "auto";
    if (typeof savedScroll === "number") {
      window.scrollTo(0, savedScroll);
    } else {
      var hash = new URL(url, window.location.href).hash;
      var target = hash && document.getElementById(decodeURIComponent(hash.slice(1)));
      if (target) target.scrollIntoView();
      else window.scrollTo(0, 0);
    }
    root.style.scrollBehavior = previous;
  }

  function saveScroll() {
    var state = Object.assign({}, history.state || {}, { fsbook: true, scrollY: window.scrollY });
    history.replaceState(state, "", window.location.href);
  }

  var scrollStatePending = false;
  function rememberScroll() {
    if (scrollStatePending) return;
    scrollStatePending = true;
    var page = pageKey(window.location.href);
    window.requestAnimationFrame(function () {
      scrollStatePending = false;
      if (pageKey(window.location.href) === page) saveScroll();
    });
  }

  function swapPage(html, url, options) {
    var parsed = new DOMParser().parseFromString(html, "text/html");
    var nextBody = parsed.querySelector(".body");
    var nextPagenav = parsed.querySelector(".pagenav");
    var body = document.querySelector(".body");
    var pagenav = document.querySelector(".pagenav");
    if (!nextBody || !nextPagenav || !body || !pagenav || !parsed.title) return false;

    resolveUrls(nextBody, url);
    resolveUrls(nextPagenav, url);
    if (options.push) history.pushState({ fsbook: true, scrollY: 0 }, "", url);

    document.title = parsed.title;
    var description = document.querySelector('meta[name="description"]');
    var nextDescription = parsed.querySelector('meta[name="description"]');
    if (description && nextDescription) description.content = nextDescription.content;
    document.body.className = parsed.body.className;
    body.replaceWith(document.adoptNode(nextBody));
    pagenav.replaceWith(document.adoptNode(nextPagenav));
    if (menuButton) menuButton.setAttribute("aria-expanded", "false");

    initPage();
    prefetchAdjacentPages();
    setScroll(url, options.savedScroll);

    if (options.focus) {
      var content = document.getElementById("content");
      if (content) {
        content.setAttribute("tabindex", "-1");
        content.focus({ preventScroll: true });
      }
    }
    return true;
  }

  function navigate(url, options) {
    var version = ++navigationVersion;
    var content = document.getElementById("content");
    if (options.push) saveScroll();
    if (content) content.setAttribute("aria-busy", "true");

    fetchPage(url)
      .then(function (html) {
        if (version !== navigationVersion) return;
        if (!swapPage(html, url, options)) throw new Error("Page shell is incomplete");
      })
      .catch(function () {
        if (version !== navigationVersion) return;
        if (options.push) window.location.assign(url);
        else window.location.reload();
      });
  }

  document.addEventListener("click", function (event) {
    var link = event.target.closest && event.target.closest("a");
    if (!link) return;

    var selection = window.getSelection();
    if (link.closest(".pagenav") && selection && !selection.isCollapsed) {
      event.preventDefault();
      return;
    }
    if (
      event.defaultPrevented ||
      event.button !== 0 ||
      event.altKey ||
      event.ctrlKey ||
      event.metaKey ||
      event.shiftKey ||
      !isBookLink(link)
    ) {
      return;
    }

    event.preventDefault();
    document.body.classList.remove("sidebar-open");
    navigate(link.href, { push: true, focus: true });
  });

  ["pointerover", "focusin"].forEach(function (eventName) {
    document.addEventListener(eventName, function (event) {
      var link = event.target.closest && event.target.closest("a");
      if (isBookLink(link)) fetchPage(link.href).catch(function () {});
    });
  });

  document.addEventListener("keydown", function (event) {
    if (event.altKey || event.ctrlKey || event.metaKey || event.shiftKey) return;
    var target = event.target;
    if (target && (target.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(target.tagName))) {
      return;
    }
    var rel = event.key === "ArrowLeft" ? "prev" : event.key === "ArrowRight" ? "next" : null;
    if (!rel) return;
    var link = document.querySelector('.pagenav a[rel="' + rel + '"]');
    if (link) {
      event.preventDefault();
      link.click();
    }
  });

  window.addEventListener("popstate", function (event) {
    navigate(window.location.href, {
      push: false,
      focus: false,
      savedScroll: event.state && event.state.fsbook ? event.state.scrollY : 0,
    });
  });
  window.addEventListener("scroll", rememberScroll, { passive: true });
  window.addEventListener("pagehide", saveScroll);

  if ("scrollRestoration" in history) history.scrollRestoration = "manual";
  history.replaceState(
    Object.assign({}, history.state || {}, { fsbook: true, scrollY: window.scrollY }),
    "",
    window.location.href
  );

  initPage();
  prefetchAdjacentPages();
})();
