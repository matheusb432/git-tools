(function(){
  // ! Scope every view to its own .layout root: the tabbed (diff subrepos) view inlines one
  // ! .layout per panel in a single document, so document.querySelector would only ever wire
  // ! the first panel. Querying within `root` keeps each tab independently interactive.
  function initView(root){
    var layout = root;
    var fileEls = [].slice.call(root.querySelectorAll('details.file'));
    var tocLinks = [].slice.call(root.querySelectorAll('nav.toc a'));
    var commitEls = [].slice.call(root.querySelectorAll('.commit[data-sha]'));
    var showAll = root.querySelector('.showall');
    var treeToggle = root.querySelector('.tree-toggle');
    var treeBody = root.querySelector('.tree-body');
    var activeSha = null;
    function esc(s){ return String(s).replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;'); }
    function shasOf(el){ return (el.getAttribute('data-commits')||'').split(' ').filter(Boolean); }
    // ---- commit filter: show only the files a commit touched ----
    function applyFilter(){
      fileEls.forEach(function(el){ el.hidden = !!activeSha && shasOf(el).indexOf(activeSha) === -1; });
      tocLinks.forEach(function(a){ a.hidden = !!activeSha && shasOf(a).indexOf(activeSha) === -1; });
      commitEls.forEach(function(c){ c.classList.toggle('active', activeSha === c.getAttribute('data-sha')); });
      if (showAll) showAll.hidden = !activeSha;
      buildTree();
    }
    commitEls.forEach(function(c){
      function toggle(){ var s = c.getAttribute('data-sha'); activeSha = (activeSha === s) ? null : s; applyFilter(); }
      c.addEventListener('click', toggle);
      c.addEventListener('keydown', function(e){ if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); toggle(); } });
    });
    if (showAll) showAll.addEventListener('click', function(){ activeSha = null; applyFilter(); });
    // ---- sidebar toggle: flips the layout state attribute; CSS reflows the rest ----
    if (treeToggle) treeToggle.addEventListener('click', function(){
      var open = layout.getAttribute('data-tree') === 'open';
      layout.setAttribute('data-tree', open ? 'closed' : 'open');
      treeToggle.setAttribute('aria-expanded', String(!open));
    });
    // ---- file tree: fold visible file paths into a nested, collapsible tree ----
    function buildTree(){
      if (!treeBody) return;
      var root = { dirs:{}, files:[] };
      fileEls.forEach(function(el){
        if (el.hidden) return;
        var parts = (el.getAttribute('data-path')||'').split('/');
        var node = root;
        for (var i = 0; i < parts.length - 1; i++){ node = node.dirs[parts[i]] = node.dirs[parts[i]] || { dirs:{}, files:[] }; }
        // ! store the element itself, not its id: file ids can collide across repos in a tabbed view
        node.files.push({ name: parts[parts.length - 1], el: el });
      });
      treeBody.innerHTML = '';
      treeBody.appendChild(renderNode(root));
    }
    function renderNode(node){
      var ul = document.createElement('ul');
      Object.keys(node.dirs).sort().forEach(function(name){
        var li = document.createElement('li'); li.className = 'tnode tdir open';
        var label = document.createElement('div'); label.className = 'tlabel';
        label.innerHTML = '<span class="tcaret"></span><span class="tname">' + esc(name) + '</span>';
        label.addEventListener('click', function(){ li.classList.toggle('open'); });
        li.appendChild(label); li.appendChild(renderNode(node.dirs[name])); ul.appendChild(li);
      });
      node.files.sort(function(a,b){ return a.name < b.name ? -1 : 1; }).forEach(function(f){
        var li = document.createElement('li'); li.className = 'tnode tfile';
        var label = document.createElement('div'); label.className = 'tlabel';
        label.innerHTML = '<span class="ticon"></span><span class="tname">' + esc(f.name) + '</span>';
        label.addEventListener('click', function(){
          var t = f.el;
          if (!t) return;
          t.open = true; t.scrollIntoView({ behavior:'smooth', block:'start' });
          t.classList.add('flash'); setTimeout(function(){ t.classList.remove('flash'); }, 1200);
        });
        li.appendChild(label); ul.appendChild(li);
      });
      return ul;
    }
    buildTree();
  }
  [].forEach.call(document.querySelectorAll('.layout'), initView);
})();
