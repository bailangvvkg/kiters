// What Almanac adds to pages that already work without it: the header over
// the banner, the reading bar and the table of contents, code blocks,
// headings, links, tables and pictures in an article, social cards, copy
// buttons, the Search plugin's buttons and what GitHub says of projects.
// The words it says come from data-* attributes on <body>.
(function () {
  "use strict";

  var root = document.documentElement;
  var body = document.body;
  var say = body.dataset;

  var ICON_COPY =
    '<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect width="14" height="14" x="8" y="8" rx="2" ry="2"/><path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2"/></svg>';
  var ICON_CHECK =
    '<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M20 6 9 17l-5-5"/></svg>';

  // Runs fn now and on every animation frame the page scrolls or resizes in.
  function onScroll(fn) {
    var ticking = false;
    function run() {
      ticking = false;
      fn();
    }
    function handler() {
      if (!ticking) {
        ticking = true;
        requestAnimationFrame(run);
      }
    }
    fn();
    window.addEventListener("scroll", handler, { passive: true });
    window.addEventListener("resize", handler);
  }

  // A page served over plain http on a LAN has no clipboard API.
  function copyText(text) {
    if (navigator.clipboard && window.isSecureContext) {
      return navigator.clipboard.writeText(text).catch(function () {
        return copyFallback(text);
      });
    }
    return copyFallback(text);
  }
  function copyFallback(text) {
    var area = document.createElement("textarea");
    area.value = text;
    area.style.position = "fixed";
    area.style.opacity = "0";
    body.appendChild(area);
    area.select();
    try {
      return document.execCommand("copy") ? Promise.resolve() : Promise.reject(new Error("copy failed"));
    } catch (e) {
      return Promise.reject(e);
    } finally {
      area.remove();
    }
  }

  // ===== Search: the plugin's, where the site has it =====
  if (window.KiteSearch) root.classList.add("has-search");

  // ===== Header over the banner fills in once the page scrolls =====
  var header = document.querySelector(".site-header[data-over-banner]");
  if (header) {
    onScroll(function () {
      header.classList.toggle("is-scrolled", window.scrollY > 96);
    });
  }

  // ===== Reading bar: once the title has gone, with how far one has read =====
  var bar = document.querySelector(".reading-bar");
  if (bar) {
    onScroll(function () {
      var y = window.scrollY;
      var max = document.documentElement.scrollHeight - window.innerHeight;
      bar.classList.toggle("is-shown", y > 280);
      bar.style.setProperty("--progress", max > 0 ? Math.min(1, Math.max(0, y / max)) : 0);
    });
  }
  document.querySelectorAll("[data-to-top]").forEach(function (el) {
    el.addEventListener("click", function () {
      window.scrollTo({ top: 0, behavior: "smooth" });
    });
  });

  // ===== The menu of a narrow screen closes on Escape =====
  var navToggle = document.getElementById("nav-toggle");
  document.addEventListener("keydown", function (e) {
    if (e.key !== "Escape") return;
    if (navToggle && navToggle.checked) navToggle.checked = false;
    closePops(null);
  });
  document.querySelectorAll(".drawer a").forEach(function (a) {
    a.addEventListener("click", function () {
      if (navToggle) navToggle.checked = false;
    });
  });

  // ===== Articles =====
  document.querySelectorAll(".prose-almanac").forEach(function (prose) {
    anchorHeadings(prose);
    markLinks(prose);
    wrapTables(prose);
    enhanceCode(prose);
    zoomPictures(prose);
  });

  function anchorHeadings(prose) {
    prose.querySelectorAll("h2[id], h3[id], h4[id]").forEach(function (h) {
      if (h.querySelector(".heading-anchor")) return;
      var a = document.createElement("a");
      a.className = "heading-anchor";
      a.href = "#" + encodeURIComponent(h.id);
      a.setAttribute("aria-label", say.anchor || "#");
      a.textContent = "#";
      h.appendChild(a);
    });
  }

  function markLinks(prose) {
    prose.querySelectorAll("a[href]").forEach(function (a) {
      if (a.classList.contains("heading-anchor") || a.querySelector("img")) return;
      var url;
      try {
        url = new URL(a.getAttribute("href"), location.href);
      } catch (e) {
        return;
      }
      if (!/^https?:$/.test(url.protocol) || url.host === location.host) return;
      a.classList.add("external");
      a.target = "_blank";
      a.rel = "noopener noreferrer";
    });
  }

  function wrapTables(prose) {
    prose.querySelectorAll("table").forEach(function (table) {
      if (table.parentElement.classList.contains("table-wrap")) return;
      var wrap = document.createElement("div");
      wrap.className = "table-wrap";
      table.parentNode.insertBefore(wrap, table);
      wrap.appendChild(table);
    });
  }

  function copyButton(code) {
    var btn = document.createElement("button");
    btn.type = "button";
    btn.className = "code-copy-btn";
    btn.title = say.copy || "Copy";
    btn.setAttribute("aria-label", say.copy || "Copy");
    btn.innerHTML = ICON_COPY;
    btn.addEventListener("click", function () {
      copyText(code.innerText.replace(/\n$/, "")).then(function () {
        btn.classList.add("copied");
        btn.innerHTML = ICON_CHECK;
        btn.title = say.copied || "Copied";
        setTimeout(function () {
          btn.classList.remove("copied");
          btn.innerHTML = ICON_COPY;
          btn.title = say.copy || "Copy";
        }, 1600);
      });
    });
    return btn;
  }

  // Kite names a fenced block's language in data-lang; such a block gets a
  // bar that names it, and any other a copy button in its corner.
  function enhanceCode(prose) {
    prose.querySelectorAll("pre").forEach(function (pre) {
      var code = pre.querySelector("code");
      if (!code || pre.dataset.enhanced) return;
      pre.dataset.enhanced = "1";
      var lang = pre.getAttribute("data-lang");
      if (lang) {
        var block = document.createElement("div");
        block.className = "code-block not-prose";
        var head = document.createElement("div");
        head.className = "code-block__bar";
        var label = document.createElement("span");
        label.className = "code-block__lang";
        label.textContent = lang;
        head.appendChild(label);
        head.appendChild(copyButton(code));
        pre.parentNode.insertBefore(block, pre);
        block.appendChild(head);
        block.appendChild(pre);
        return;
      }
      var host = document.createElement("div");
      host.className = "code-copy-host";
      pre.parentNode.insertBefore(host, pre);
      host.appendChild(pre);
      host.appendChild(copyButton(code));
    });
  }

  function zoomPictures(prose) {
    var pictures = Array.prototype.filter.call(prose.querySelectorAll("img"), function (img) {
      return !img.closest("a, .not-prose, .pic-grid");
    });
    pictures.forEach(function (img, i) {
      img.setAttribute("data-zoom", "");
      img.addEventListener("click", function () {
        openViewer(
          pictures.map(function (p) {
            return { src: p.currentSrc || p.src, alt: p.alt };
          }),
          i
        );
      });
    });
  }

  // ===== Picture viewer =====
  var viewer, viewerImg, viewerCaption, viewerItems = [], viewerAt = 0;

  function viewerButton(cls, label, path) {
    var btn = document.createElement("button");
    btn.type = "button";
    btn.className = "lightbox__btn " + cls;
    btn.setAttribute("aria-label", label);
    btn.innerHTML =
      '<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">' +
      path +
      "</svg>";
    return btn;
  }

  function buildViewer() {
    viewer = document.createElement("dialog");
    viewer.className = "lightbox";
    var stage = document.createElement("div");
    stage.className = "lightbox__stage";
    viewerImg = document.createElement("img");
    viewerImg.className = "lightbox__img";
    viewerImg.alt = "";
    viewerCaption = document.createElement("p");
    viewerCaption.className = "lightbox__caption";
    stage.appendChild(viewerImg);
    stage.appendChild(viewerCaption);
    viewer.appendChild(stage);

    var close = viewerButton("lightbox__close", say.close || "Close", '<path d="M18 6 6 18"/><path d="m6 6 12 12"/>');
    var prev = viewerButton("lightbox__prev", say.imagePrev || "Previous", '<path d="m15 18-6-6 6-6"/>');
    var next = viewerButton("lightbox__next", say.imageNext || "Next", '<path d="m9 18 6-6-6-6"/>');
    close.addEventListener("click", function () {
      viewer.close();
    });
    prev.addEventListener("click", function () {
      showPicture(viewerAt - 1);
    });
    next.addEventListener("click", function () {
      showPicture(viewerAt + 1);
    });
    viewer.appendChild(close);
    viewer.appendChild(prev);
    viewer.appendChild(next);
    viewer.prevButton = prev;
    viewer.nextButton = next;

    stage.addEventListener("click", function (e) {
      if (e.target === stage) viewer.close();
    });
    viewer.addEventListener("keydown", function (e) {
      if (e.key === "ArrowLeft") showPicture(viewerAt - 1);
      else if (e.key === "ArrowRight") showPicture(viewerAt + 1);
    });
    viewer.addEventListener("close", function () {
      body.style.overflow = "";
    });
    body.appendChild(viewer);
  }

  function showPicture(i) {
    var n = viewerItems.length;
    viewerAt = (i + n) % n;
    var item = viewerItems[viewerAt];
    viewerImg.src = item.src;
    viewerImg.alt = item.alt || "";
    viewerCaption.textContent = item.alt || "";
    viewerCaption.hidden = !item.alt;
  }

  function openViewer(items, i) {
    if (!items.length) return;
    if (!viewer) buildViewer();
    viewerItems = items;
    viewer.prevButton.hidden = viewer.nextButton.hidden = items.length < 2;
    showPicture(i);
    if (!viewer.open) {
      viewer.showModal();
      body.style.overflow = "hidden";
    }
  }

  // A group of pictures that are links to themselves, as a moment's.
  document.querySelectorAll("[data-gallery]").forEach(function (group) {
    var links = Array.prototype.slice.call(group.querySelectorAll("a[href]"));
    links.forEach(function (a, i) {
      a.addEventListener("click", function (e) {
        if (e.metaKey || e.ctrlKey || e.shiftKey) return;
        e.preventDefault();
        openViewer(
          links.map(function (l) {
            var img = l.querySelector("img");
            return { src: l.href, alt: img ? img.alt : "" };
          }),
          i
        );
      });
    });
  });

  // ===== Table of contents follows the heading being read =====
  var tocLinks = Array.prototype.slice.call(document.querySelectorAll(".toc-link"));
  if (tocLinks.length) {
    var targets = tocLinks
      .map(function (a) {
        return document.getElementById(decodeURIComponent(a.hash.slice(1)));
      })
      .filter(Boolean);
    if (targets.length) {
      onScroll(function () {
        var y = window.scrollY + 120;
        var current = targets[0].id;
        for (var i = 0; i < targets.length; i++) {
          if (targets[i].getBoundingClientRect().top + window.scrollY <= y) current = targets[i].id;
          else break;
        }
        if (window.innerHeight + window.scrollY >= document.documentElement.scrollHeight - 2) {
          current = targets[targets.length - 1].id;
        }
        tocLinks.forEach(function (a) {
          a.classList.toggle("is-active", decodeURIComponent(a.hash.slice(1)) === current);
        });
      });
    }
  }

  // ===== Social cards: one open at a time, closed from outside =====
  var pops = Array.prototype.slice.call(document.querySelectorAll("details.social-pop"));
  function closePops(except) {
    pops.forEach(function (d) {
      if (d !== except) d.open = false;
    });
  }
  pops.forEach(function (d) {
    d.addEventListener("toggle", function () {
      if (d.open) closePops(d);
    });
  });
  document.addEventListener("click", function (e) {
    if (!e.target.closest || !e.target.closest("details.social-pop")) closePops(null);
  });

  // ===== Copy buttons: data-copy-from names the element to copy =====
  document.querySelectorAll("[data-copy-from]").forEach(function (btn) {
    btn.addEventListener("click", function () {
      var from = document.getElementById(btn.getAttribute("data-copy-from"));
      if (!from) return;
      var text = "value" in from ? from.value : from.textContent;
      copyText(text.trim()).then(function () {
        btn.classList.add("copied");
        var label = btn.querySelector(".copy-label");
        var was = label ? label.textContent : "";
        if (label) label.textContent = say.copied || "Copied";
        setTimeout(function () {
          btn.classList.remove("copied");
          if (label) label.textContent = was;
        }, 1500);
      });
    });
  });

  // ===== Projects on GitHub, fetched by the reader's browser =====
  // Only under an element marked data-github-user: each repository's
  // numbers, what a project leaves out (its summary, tech and site), and the
  // user's other repositories where a projects page asks for them.
  var shelf = document.querySelector("[data-github-user]");
  if (shelf) github(shelf, shelf.getAttribute("data-github-user"));

  function github(shelf, user) {
    var cards = [].slice.call(shelf.querySelectorAll("[data-repo]"));
    var more = shelf.querySelector("[data-gh-more]");
    if (!user || (!cards.length && !more)) return;
    var API = "https://api.github.com";
    var owner = user.toLowerCase();
    var SLUG = /^[\w.-]+\/[\w.-]+$/;

    // An hour keeps a reader well under GitHub's 60 anonymous calls an hour.
    function load(key, url, read) {
      try {
        var hit = JSON.parse(localStorage.getItem(key) || "null");
        if (hit && Date.now() - hit.at < 3600 * 1000) return Promise.resolve(hit.data);
      } catch (e) {}
      return fetch(url, { headers: { Accept: "application/vnd.github+json" } })
        .then(function (r) {
          if (!r.ok) throw new Error(r.status);
          return r.json();
        })
        .then(function (json) {
          var data = read(json);
          try {
            localStorage.setItem(key, JSON.stringify({ at: Date.now(), data: data }));
          } catch (e) {}
          return data;
        });
    }
    function repo(r) {
      var license = r.license && r.license.spdx_id;
      return {
        name: r.name,
        full: String(r.full_name),
        url: r.html_url,
        description: r.description || "",
        stars: r.stargazers_count || 0,
        forks: r.forks_count || 0,
        language: r.language || "",
        topics: r.topics || [],
        homepage: r.homepage || "",
        license: license && license !== "NOASSERTION" ? license : "",
        pushed: r.pushed_at || "",
        archived: !!r.archived,
        fork: !!r.fork,
      };
    }

    load(
      "almanac-github-v2:" + owner,
      API + "/users/" + encodeURIComponent(user) + "/repos?per_page=100&type=owner&sort=pushed",
      function (list) {
        return Array.isArray(list) ? list.map(repo) : [];
      }
    )
      .then(function (list) {
        var mine = {};
        list.forEach(function (r) {
          mine[r.full.toLowerCase()] = r;
        });
        // A repository of another owner is asked for by itself, a few at most.
        var asked = {};
        cards.forEach(function (card) {
          var slug = card.getAttribute("data-repo").toLowerCase();
          if (mine[slug]) return fill(card, mine[slug]);
          if (!SLUG.test(slug)) return;
          if (!asked[slug]) {
            if (Object.keys(asked).length >= 8) return;
            asked[slug] = load("almanac-github-v2:" + slug, API + "/repos/" + slug, repo);
          }
          asked[slug]
            .then(function (r) {
              fill(card, r);
            })
            .catch(function () {});
        });
        if (more) listMore(more, list, cards);
      })
      .catch(function () {});

    function count(n) {
      return n.toLocaleString();
    }
    function web(url) {
      return /^https?:\/\//.test(url) ? url : "";
    }
    function chip(text) {
      var el = document.createElement("span");
      el.className = "chip";
      el.textContent = text;
      return el;
    }

    function fill(card, r) {
      var row = card.querySelector("[data-gh-row]");
      if (row) {
        var shown = false;
        var set = function (name, value) {
          var slot = row.querySelector('[data-gh="' + name + '"]');
          if (!slot) return;
          slot.hidden = !value;
          if (!value) return;
          (slot.querySelector("[data-gh-value]") || slot).textContent = value;
          shown = true;
        };
        set("stars", r.stars ? count(r.stars) : "");
        set("forks", r.forks ? count(r.forks) : "");
        set("language", r.language);
        set("pushed", r.pushed.slice(0, 7));
        row.hidden = !shown;
      }
      var facts = {
        language: r.language,
        stars: count(r.stars),
        forks: count(r.forks),
        license: r.license,
        pushed: r.pushed.slice(0, 10),
      };
      card.querySelectorAll("[data-gh-fact]").forEach(function (fact) {
        var value = facts[fact.getAttribute("data-gh-fact")];
        if (!value) return;
        fact.querySelector("[data-gh-value]").textContent = value;
        fact.hidden = false;
      });
      var summary = card.querySelector('[data-gh-fill="summary"]');
      if (summary && r.description) {
        summary.textContent = r.description;
        summary.hidden = false;
      }
      var tech = card.querySelector('[data-gh-fill="tech"]');
      var words = r.topics.length ? r.topics.slice(0, 6) : r.language ? [r.language] : [];
      if (tech && words.length && !tech.children.length) {
        words.forEach(function (w) {
          tech.appendChild(chip(w));
        });
        tech.hidden = false;
      }
      var home = card.querySelector('[data-gh-fill="homepage"]');
      if (home && web(r.homepage)) {
        home.href = r.homepage;
        home.hidden = false;
      }
    }

    // The user's other repositories: no forks, none already listed, and not
    // the one that only holds their profile.
    function listMore(more, list, cards) {
      var listed = {};
      cards.forEach(function (card) {
        listed[card.getAttribute("data-repo").toLowerCase()] = true;
      });
      var byPush = more.getAttribute("data-gh-sort") === "pushed";
      var limit = parseInt(more.getAttribute("data-gh-limit"), 10) || 6;
      var picked = list
        .filter(function (r) {
          var name = r.name.toLowerCase();
          return !r.fork && !listed[r.full.toLowerCase()] && name !== owner && name !== ".github";
        })
        .sort(function (a, b) {
          if (!byPush && b.stars !== a.stars) return b.stars - a.stars;
          return a.pushed < b.pushed ? 1 : a.pushed > b.pushed ? -1 : 0;
        })
        .slice(0, limit);
      var template = more.querySelector("template[data-gh-card]");
      var grid = more.querySelector("[data-gh-list]");
      if (!picked.length || !template || !grid) return;
      picked.forEach(function (r) {
        var card = template.content.firstElementChild.cloneNode(true);
        var slot = function (name) {
          return card.querySelector('[data-gh-slot="' + name + '"]');
        };
        var name = slot("name");
        name.textContent = r.name;
        if (/^https:\/\/github\.com\//.test(r.url)) name.href = r.url;
        slot("archived").hidden = !r.archived;
        if (r.description) slot("description").textContent = r.description;
        else slot("description").remove();
        if (r.topics.length) {
          r.topics.slice(0, 4).forEach(function (w) {
            slot("topics").appendChild(chip(w));
          });
        } else slot("topics").remove();
        slot("stars").textContent = count(r.stars);
        slot("forks").textContent = count(r.forks);
        if (r.language) slot("language").textContent = r.language;
        else slot("language-row").remove();
        slot("pushed").textContent = r.pushed.slice(0, 7);
        grid.appendChild(card);
      });
      var n = more.querySelector("[data-gh-count]");
      if (n) n.textContent = picked.length;
      more.hidden = false;
    }
  }
})();
