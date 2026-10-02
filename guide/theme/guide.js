// Guide chrome, progressive navigation, and code sandboxes.

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
        localStorage.setItem("full-stack-guide-theme", next);
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

  function initSidebar(savedScroll) {
    var active = document.querySelector('.chapter-list a[aria-current="page"]');
    var sidebar = document.getElementById("sidebar");
    if (!active || !sidebar) return;

    if (typeof savedScroll === "number") {
      sidebar.scrollTop = savedScroll;
      var top = active.offsetTop;
      var bottom = top + active.offsetHeight;
      if (top < sidebar.scrollTop + 16) {
        sidebar.scrollTop = Math.max(0, top - 16);
      } else if (bottom > sidebar.scrollTop + sidebar.clientHeight - 16) {
        sidebar.scrollTop = bottom - sidebar.clientHeight + 16;
      }
    } else if (active.offsetTop > sidebar.clientHeight - 80) {
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
      var line = window.innerHeight / 2 + 1;
      var found = targets[0];
      targets.forEach(function (entry) {
        if (entry.node.getBoundingClientRect().top <= line) found = entry;
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
    var url = new URL("../sandbox/full_stack_guide_wasm.js", base);
    runtime = import(url.href).then(function (module) {
      return module.default().then(function () {
        return module;
      });
    });
    return runtime;
  }

  function sourceText(node) {
    var value = "value" in node ? node.value : node.textContent;
    return value.replace(/\r\n?/g, "\n").replace(/ /g, " ");
  }

  function lineStarts(source, start, end) {
    var first = source.lastIndexOf("\n", start - 1) + 1;
    var limit = end > start && source.charAt(end - 1) === "\n" ? end - 1 : end;
    var starts = [first];
    var newline = source.indexOf("\n", first);
    while (newline >= 0 && newline < limit) {
      starts.push(newline + 1);
      newline = source.indexOf("\n", newline + 1);
    }
    return starts;
  }

  function remapOffset(offset, edits) {
    var delta = 0;
    for (var i = 0; i < edits.length; i += 1) {
      var edit = edits[i];
      if (offset < edit.at) break;
      if (edit.remove && offset <= edit.at + edit.remove) {
        return edit.at + delta + edit.insert.length;
      }
      delta += edit.insert.length - edit.remove;
    }
    return offset + delta;
  }

  function applyTabEdit(editor, outdent) {
    var TAB = 4;
    var source = sourceText(editor);
    var start = editor.selectionStart;
    var end = editor.selectionEnd;

    // The IDE indents with real spaces aligned to multiples of four, so the
    // sandbox does the same: code copied between the page and the editor keeps
    // the same indentation, and backspace removes one space at a time.
    if (!outdent && start === end) {
      var lineStart = source.lastIndexOf("\n", start - 1) + 1;
      var spaces = TAB - ((start - lineStart) % TAB);
      editor.setRangeText(" ".repeat(spaces), start, end, "end");
      editor.dispatchEvent(new Event("input", { bubbles: true }));
      return;
    }

    var edits = lineStarts(source, start, end).map(function (at) {
      if (!outdent) return { at: at, remove: 0, insert: " ".repeat(TAB) };
      if (source.charAt(at) === "\t") return { at: at, remove: 1, insert: "" };
      var count = source.slice(at).match(/^ +/);
      count = count ? count[0].length : 0;
      if (!count) return null;
      var remove = count % TAB === 0 ? Math.min(TAB, count) : count % TAB;
      return { at: at, remove: remove, insert: "" };
    }).filter(function (edit) {
      return edit && (edit.insert || edit.remove);
    });
    if (!edits.length) return;

    var updated = source;
    edits.slice().reverse().forEach(function (edit) {
      updated = updated.slice(0, edit.at) + edit.insert + updated.slice(edit.at + edit.remove);
    });
    editor.value = updated;
    editor.setSelectionRange(remapOffset(start, edits), remapOffset(end, edits));
    editor.dispatchEvent(new Event("input", { bubbles: true }));
  }

  function expectedRegisters(raw) {
    var expected = {};
    (raw || "").split("|").forEach(function (item) {
      var separator = item.indexOf(":");
      if (separator < 1) return;
      var name = item.slice(0, separator);
      try {
        expected[name] = BigInt(item.slice(separator + 1));
      } catch (e) {
        return;
      }
    });
    return expected;
  }

  function resultElement(tag, className, text) {
    var node = document.createElement(tag);
    if (className) node.className = className;
    if (text !== undefined) node.textContent = text;
    return node;
  }

  function showResult(output, result, sandbox) {
    output.hidden = false;
    output.textContent = "";

    if (result.error) {
      var error = document.createElement("div");
      error.className = "err";
      error.textContent = result.error;
      output.appendChild(error);
      return;
    }

    // A sandbox can opt out of the register dump with registers=false in its
    // fence info; the dump is shown by default.
    var showRegisters = sandbox.dataset.registers !== "false";
    var expected = expectedRegisters(sandbox.dataset.expect);
    var expectedNames = Object.keys(expected);
    var registers = (result.registers || [])
      .map(function (entry) {
        try {
          return { name: entry.name, value: BigInt(entry.value) };
        } catch (e) {
          return null;
        }
      })
      .filter(function (entry) {
        return (
          entry &&
          (entry.value !== BigInt(0) ||
            Object.prototype.hasOwnProperty.call(expected, entry.name))
        );
      });

    var heading = resultElement("div", "sandbox-result-heading");
    var title = resultElement("span", "sandbox-result-title", "Run result");
    var status = "Finished";
    if (result.exit_code !== null && result.exit_code !== undefined) {
      status = "Exited " + result.exit_code;
    } else if (result.max_steps_reached) {
      status = "Stopped at step limit";
    }
    title.appendChild(resultElement("span", "sandbox-result-status", status));
    heading.appendChild(title);

    if (showRegisters) {
      var radix = resultElement("div", "sandbox-radix");
      radix.setAttribute("role", "group");
      radix.setAttribute("aria-label", "Register number format");
      var decimalButton = resultElement("button", "active", "Decimal");
      var hexButton = resultElement("button", "", "Hex");
      decimalButton.type = "button";
      hexButton.type = "button";
      decimalButton.setAttribute("aria-pressed", "true");
      hexButton.setAttribute("aria-pressed", "false");
      radix.appendChild(decimalButton);
      radix.appendChild(hexButton);
      heading.appendChild(radix);
    }
    output.appendChild(heading);

    if (result.uart) {
      var uartSection = resultElement("section", "sandbox-result-section");
      uartSection.appendChild(resultElement("div", "sandbox-output-label", "Program output"));
      uartSection.appendChild(resultElement("pre", "sandbox-uart", result.uart));
      output.appendChild(uartSection);
    }

    if (showRegisters) {
      var registerSection = resultElement("section", "sandbox-result-section");
      registerSection.appendChild(
        resultElement("div", "sandbox-output-label", "Nonzero registers")
      );
      var valueCells = [];
      var expectedCells = [];
      if (registers.length) {
        var table = resultElement("table", "sandbox-registers");
        var head = document.createElement("thead");
        var headerRow = document.createElement("tr");
        ["Register", "Value"].concat(expectedNames.length ? ["Expected"] : []).forEach(function (label) {
          headerRow.appendChild(resultElement("th", "", label));
        });
        head.appendChild(headerRow);
        table.appendChild(head);
        var body = document.createElement("tbody");

        registers.forEach(function (entry) {
          var row = document.createElement("tr");
          row.appendChild(resultElement("th", "sandbox-register-name", entry.name));
          var valueCell = resultElement("td", "sandbox-register-value");
          row.appendChild(valueCell);
          valueCells.push({ node: valueCell, value: entry.value });

          if (expectedNames.length) {
            var expectedCell = document.createElement("td");
            if (Object.prototype.hasOwnProperty.call(expected, entry.name)) {
              var matches = entry.value === expected[entry.name];
              expectedCell.className = matches ? "expected-match" : "expected-mismatch";
              expectedCells.push({
                node: expectedCell,
                value: expected[entry.name],
                suffix: matches ? " match" : " expected",
              });
            } else {
              expectedCell.textContent = "-";
            }
            row.appendChild(expectedCell);
          }
          body.appendChild(row);
        });
        table.appendChild(body);
        registerSection.appendChild(table);
      } else {
        registerSection.appendChild(
          resultElement("div", "sandbox-empty-registers", "No nonzero registers.")
        );
      }
      output.appendChild(registerSection);

      function display(value, format) {
        return format === "hex" ? "0x" + value.toString(16) : value.toString(10);
      }

      function setRadix(format) {
        var decimal = format === "decimal";
        decimalButton.classList.toggle("active", decimal);
        hexButton.classList.toggle("active", !decimal);
        decimalButton.setAttribute("aria-pressed", decimal ? "true" : "false");
        hexButton.setAttribute("aria-pressed", decimal ? "false" : "true");
        valueCells.forEach(function (cell) {
          cell.node.textContent = display(cell.value, format);
        });
        expectedCells.forEach(function (cell) {
          cell.node.textContent = display(cell.value, format) + cell.suffix;
        });
      }

      decimalButton.addEventListener("click", function () { setRadix("decimal"); });
      hexButton.addEventListener("click", function () { setRadix("hex"); });
      setRadix("decimal");
    }

    output.appendChild(
      resultElement("div", "sandbox-result-meta", result.steps + " steps | pc " + result.pc)
    );
  }

  function initSandboxes() {
    var sandboxes = Array.prototype.slice.call(document.querySelectorAll(".sandbox"));
    sandboxes.forEach(function (sandbox) {
      var code = sandbox.querySelector(".sandbox-code");
      var highlight = sandbox.querySelector(".sandbox-highlight");
      var output = sandbox.querySelector(".sandbox-output");
      var runButton = sandbox.querySelector(".sandbox-run");
      var resetButton = sandbox.querySelector(".sandbox-reset");
      var pristine = code.value;
      var highlighter = null;

      function repaint(module, source) {
        try {
          highlight.innerHTML = module.highlight_html(sandbox.dataset.lang || "asm", source);
        } catch (e) {
          highlight.textContent = source;
        }
      }

      loadSandbox()
        .then(function (module) {
          highlighter = module;
          repaint(module, sourceText(code));
        })
        .catch(function () {});

      code.addEventListener("pointerdown", function () {
        if (document.activeElement !== code) code.focus({ preventScroll: true });
      });

      code.addEventListener("input", function () {
        var source = sourceText(code);
        if (highlighter) repaint(highlighter, source);
        else highlight.textContent = source;
        code.scrollLeft = 0;
        code.scrollTop = 0;
      });

      code.addEventListener("keydown", function (event) {
        if (event.key !== "Tab" || event.altKey || event.ctrlKey || event.metaKey) return;
        event.preventDefault();
        applyTabEdit(code, event.shiftKey);
      });

      runButton.addEventListener("click", function () {
        runButton.disabled = true;
        output.hidden = false;
        output.textContent = "Running...";

        loadSandbox()
          .then(function (module) {
            var source = sourceText(code);
            var language = sandbox.dataset.lang || "asm";
            var bare = sandbox.dataset.wrap === "bare";
            var result = module.run_code(language, source, 100000, bare);
            showResult(output, result, sandbox);
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
        code.value = pristine;
        if (highlighter) repaint(highlighter, pristine);
        else highlight.textContent = pristine;
        code.scrollTop = 0;
        code.scrollLeft = 0;
        output.hidden = true;
        output.textContent = "";
      });
    });
  }

  function initPage(sidebarScroll) {
    initSidebar(sidebarScroll);
    initOutline();
    initSandboxes();
  }

  // --- Navigation cache ---
  function fetchPage(url) {
    var key = pageKey(url);
    if (pageCache.has(key)) return pageCache.get(key);

    var request = fetch(key, {
      credentials: "same-origin",
      headers: { Accept: "text/html" },
    })
      .then(function (response) {
        if (!response.ok) throw new Error("Page request failed with " + response.status);
        var type = response.headers.get("content-type") || "";
        if (!type.toLowerCase().startsWith("text/html")) {
          throw new Error("Page request returned " + (type || "an unknown media type"));
        }
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

  function isGuideLink(link) {
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
    var state = Object.assign({}, history.state || {}, { fullStackGuide: true, scrollY: window.scrollY });
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
    var sidebar = document.getElementById("sidebar");
    var sidebarScroll = sidebar ? sidebar.scrollTop : null;

    resolveUrls(nextBody, url);
    resolveUrls(nextPagenav, url);
    if (options.push) history.pushState({ fullStackGuide: true, scrollY: 0 }, "", url);

    document.title = parsed.title;
    var description = document.querySelector('meta[name="description"]');
    var nextDescription = parsed.querySelector('meta[name="description"]');
    if (description && nextDescription) description.content = nextDescription.content;
    var canonical = document.querySelector('link[rel="canonical"]');
    var nextCanonical = parsed.querySelector('link[rel="canonical"]');
    if (canonical && nextCanonical) canonical.href = new URL(nextCanonical.getAttribute("href"), url).href;
    document.body.className = parsed.body.className;
    body.replaceWith(document.adoptNode(nextBody));
    pagenav.replaceWith(document.adoptNode(nextPagenav));
    if (menuButton) menuButton.setAttribute("aria-expanded", "false");

    initPage(sidebarScroll);
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
      !isGuideLink(link)
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
      if (isGuideLink(link)) fetchPage(link.href).catch(function () {});
    });
  });

  document.addEventListener("keydown", function (event) {
    if (event.defaultPrevented || event.altKey || event.ctrlKey || event.metaKey || event.shiftKey) return;
    var target = event.target;
    if (target && (target.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(target.tagName))) {
      return;
    }
    if (target && target.closest && target.closest(".figure-viewport")) return;
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
      savedScroll: event.state && event.state.fullStackGuide ? event.state.scrollY : 0,
    });
  });
  window.addEventListener("scroll", rememberScroll, { passive: true });
  window.addEventListener("pagehide", saveScroll);

  if ("scrollRestoration" in history) history.scrollRestoration = "manual";
  history.replaceState(
    Object.assign({}, history.state || {}, { fullStackGuide: true, scrollY: window.scrollY }),
    "",
    window.location.href
  );

  initPage();
  prefetchAdjacentPages();
})();
