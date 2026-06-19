(function(){
  // ! Scope every view to its own .layout root: the tabbed (diff subrepos) view inlines one
  // ! .layout per panel in a single document, so document.querySelector would only ever wire
  // ! the first panel. Querying within `root` keeps each tab independently interactive.

  // ! Clipboard can be blocked on file:// (origin null) — mirror CopyButton's textarea +
  // ! execCommand fallback so commit-card hash copy works from a local artifact.
  function copyText(text){
    if (navigator.clipboard && navigator.clipboard.writeText) {
      return navigator.clipboard.writeText(text).then(function(){ return true; }, function(){ return execCopy(text); });
    }
    return Promise.resolve(execCopy(text));
  }
  function execCopy(text){
    try {
      var ta = document.createElement('textarea');
      ta.value = text; ta.style.position = 'fixed'; ta.style.opacity = '0';
      document.body.appendChild(ta); ta.select();
      var ok = document.execCommand('copy');
      document.body.removeChild(ta);
      return ok;
    } catch (e) { return false; }
  }

  function initView(root){
    var fileEls = [].slice.call(root.querySelectorAll('details.file'));
    var clineEls = [].slice.call(root.querySelectorAll('.cline[data-sha]'));
    var treeBody = root.querySelector('.tree-body');
    var filterInput = root.querySelector('.search input');
    var foldAll = root.querySelector('.foldall');
    var viewToggle = root.querySelector('.view-toggle');
    var activeSha = null;
    var filterText = '';
    function esc(s){ return String(s).replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;'); }
    function shasOf(el){ return (el.getAttribute('data-commits')||'').split(' ').filter(Boolean); }
    function matchesFilter(el){ return !filterText || (el.getAttribute('data-path')||'').toLowerCase().indexOf(filterText) !== -1; }

    // ---- commit filter + name filter: a file shows only if it survives both ----
    function applyFilter(){
      fileEls.forEach(function(el){
        var byCommit = !!activeSha && shasOf(el).indexOf(activeSha) === -1;
        el.hidden = byCommit || !matchesFilter(el);
      });
      syncBeads();
      buildTree();
    }

    if (filterInput) filterInput.addEventListener('input', function(){
      filterText = filterInput.value.trim().toLowerCase();
      applyFilter();
    });

    if (foldAll) foldAll.addEventListener('click', function(){
      var anyOpen = fileEls.some(function(el){ return el.open; });
      fileEls.forEach(function(el){ el.open = !anyOpen; });
    });

    function setFullMode(on){
      fileEls.forEach(function(el){
        var compact = el.querySelector('.diff-compact');
        var full = el.querySelector('.diff-full');
        if (!compact) return;
        if (!full) {
          compact.hidden = false;
          return;
        }
        compact.hidden = on;
        full.hidden = !on;
      });
      if (viewToggle) {
        viewToggle.setAttribute('aria-pressed', on ? 'true' : 'false');
        viewToggle.classList.toggle('active', on);
      }
    }

    if (viewToggle) viewToggle.addEventListener('click', function(){
      setFullMode(viewToggle.getAttribute('aria-pressed') !== 'true');
    });

    function bindHorizontalWheel(scroller){
      scroller.addEventListener('wheel', function(e){
        var max = scroller.scrollWidth - scroller.clientWidth;
        if (max <= 0 || e.ctrlKey) return;
        e.stopPropagation();
        e.preventDefault();
        var delta = Math.abs(e.deltaY) >= Math.abs(e.deltaX) ? e.deltaY : e.deltaX;
        if (!delta) return;
        var next = Math.max(0, Math.min(max, scroller.scrollLeft + delta));
        if (next === scroller.scrollLeft) return;
        scroller.scrollLeft = next;
      }, { passive: false });
    }
    [].forEach.call(root.querySelectorAll('.diff'), bindHorizontalWheel);

    // ---- file tree: fold visible files into a nested, collapsible tree.
    // ! Follow DOM/server order (already tree-sorted in render) — DON'T re-sort, so the
    // ! sidebar matches the center pane exactly. ----
    function buildTree(){
      if (!treeBody) return;
      var treeRoot = { dirs:{}, dirOrder:[], files:[] };
      fileEls.forEach(function(el){
        if (el.hidden) return;
        var parts = (el.getAttribute('data-path')||'').split('/');
        var node = treeRoot;
        for (var i = 0; i < parts.length - 1; i++){
          if (!node.dirs[parts[i]]) { node.dirs[parts[i]] = { dirs:{}, dirOrder:[], files:[] }; node.dirOrder.push(parts[i]); }
          node = node.dirs[parts[i]];
        }
        node.files.push({
          name: parts[parts.length - 1],
          el: el,
          status: el.getAttribute('data-status') || 'modified',
          statusCode: el.getAttribute('data-status-code') || 'M',
          statusLabel: el.getAttribute('data-status-label') || 'Modified file'
        });
      });
      treeBody.innerHTML = '';
      treeBody.appendChild(renderNode(treeRoot));
    }
    function renderNode(node){
      var ul = document.createElement('ul');
      node.dirOrder.forEach(function(name){
        var li = document.createElement('li'); li.className = 'tnode tdir open';
        var label = document.createElement('div'); label.className = 'tlabel';
        label.innerHTML = '<span class="tcaret"></span><span class="tname">' + esc(name) + '</span>';
        label.addEventListener('click', function(){ li.classList.toggle('open'); });
        li.appendChild(label); li.appendChild(renderNode(node.dirs[name])); ul.appendChild(li);
      });
      node.files.forEach(function(f){
        var li = document.createElement('li'); li.className = 'tnode tfile status-' + f.status; li.setAttribute('data-target', f.el.id);
        var label = document.createElement('div'); label.className = 'tlabel';
        var status = document.createElement('span');
        status.className = 'tstatus status-' + f.status;
        status.textContent = f.statusCode;
        status.title = f.statusLabel;
        status.setAttribute('aria-label', f.statusLabel);
        var name = document.createElement('span');
        name.className = 'tname';
        name.textContent = f.name;
        label.appendChild(name);
        label.appendChild(status);
        label.addEventListener('click', function(){
          var t = f.el;
          if (!t) return;
          t.open = true; t.scrollIntoView({ behavior:'smooth', block:'start' });
          t.classList.add('flash'); setTimeout(function(){ t.classList.remove('flash'); }, 1200);
          markCurrent(t);
        });
        li.appendChild(label); ul.appendChild(li);
      });
      return ul;
    }
    // ! mark the active file's sidebar leaf so the tree shows where you are (j/k + tree-click).
    function markCurrent(el){
      if (!treeBody || !el) return;
      [].forEach.call(treeBody.querySelectorAll('.tfile'), function(li){
        li.classList.toggle('cur', li.getAttribute('data-target') === el.id);
      });
    }

    // ---- commit shelf cards: card click filters files; hash tag click copies the hash;
    // hover a card WITH notes shows its native popover (top-layer escapes the shelf scroll clip). ----
    function syncBeads(){
      clineEls.forEach(function(c){
        var on = activeSha === c.getAttribute('data-sha');
        c.classList.toggle('active', on);
      });
    }
    clineEls.forEach(function(c){
      var sha = c.getAttribute('data-sha');
      c.addEventListener('click', function(){
        activeSha = (activeSha === sha) ? null : sha;
        applyFilter();
      });
      c.addEventListener('keydown', function(e){
        if (e.target.closest && e.target.closest('.sha')) return;
        if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); c.click(); }
      });

      var hashCopy = c.querySelector('.sha');
      if (hashCopy) hashCopy.addEventListener('click', function(e){
        e.stopPropagation();
        copyText(sha);
        c.classList.add('copied'); setTimeout(function(){ c.classList.remove('copied'); }, 900);
      });

      var popId = c.getAttribute('data-pop');
      if (!popId) return;
      var pop = root.querySelector('#' + (window.CSS && CSS.escape ? CSS.escape(popId) : popId));
      if (!pop) return;
      var t;
      function show(){
        clearTimeout(t);
        var r = c.getBoundingClientRect();
        var left = r.left - 338;
        if (left < 8) left = r.right + 6;
        pop.style.left = left + 'px';
        pop.style.top = Math.min(r.top, innerHeight - 210) + 'px';
        if (pop.showPopover) pop.showPopover();
      }
      function hide(){ t = setTimeout(function(){ if (pop.hidePopover) pop.hidePopover(); }, 140); }
      c.addEventListener('mouseenter', show); c.addEventListener('mouseleave', hide);
      pop.addEventListener('mouseenter', function(){ clearTimeout(t); }); pop.addEventListener('mouseleave', hide);
    });

    // ---- keyboard (scoped): / focus filter, j/k next/prev file, alt+shift+c fold all ----
    var curFile = -1;
    function focusFile(i){
      var visible = fileEls.filter(function(el){ return !el.hidden; });
      if (!visible.length) return;
      curFile = Math.max(0, Math.min(i, visible.length - 1));
      var t = visible[curFile];
      t.open = true; t.scrollIntoView({ behavior:'smooth', block:'start' });
      t.classList.add('flash'); setTimeout(function(){ t.classList.remove('flash'); }, 800);
      markCurrent(t);
    }
    // ! Listen on document (a div gets no keydown without focus) but ignore events while this
    // ! layout's tabbed panel is hidden, so each panel stays independently driven.
    document.addEventListener('keydown', function(e){
      var panel = root.closest('.panel');
      if (panel && panel.hidden) return;
      var tag = (e.target.tagName||'').toLowerCase();
      if (tag === 'input' || tag === 'textarea' || tag === 'select') {
        if (e.key === 'Escape') e.target.blur();
        return;
      }
      // ! alt+shift+c folds all (e.code is layout-independent); plain c is left free so
      // ! ctrl+c copies a selection without collapsing every file.
      if (e.altKey && e.shiftKey && e.code === 'KeyC') { e.preventDefault(); if (foldAll) foldAll.click(); }
      else if (e.key === '/') { e.preventDefault(); if (filterInput) filterInput.focus(); }
      else if (e.key === 'j') { e.preventDefault(); focusFile(curFile + 1); }
      else if (e.key === 'k') { e.preventDefault(); focusFile(curFile - 1); }
    });

    buildTree();
    setFullMode(false);
  }
  [].forEach.call(document.querySelectorAll('.layout'), initView);
})();
