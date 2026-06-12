use crate::model::{FileDiff, View};

pub fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

pub fn slug(s: &str) -> String {
    let mut body = String::new();
    let mut last_dash = false;

    for ch in s.chars() {
        if ch.is_ascii_alphanumeric() {
            body.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash {
            body.push('-');
            last_dash = true;
        }
    }

    while body.starts_with('-') {
        body.remove(0);
    }
    while body.ends_with('-') {
        body.pop();
    }

    format!("f-{body}")
}

pub fn render_diff_lines(lines: &[String]) -> String {
    let mut old_no = 0u32;
    let mut new_no = 0u32;
    let mut rows = String::new();

    for raw in lines {
        if raw.is_empty() {
            continue;
        }

        if is_meta_line(raw) {
            rows.push_str(&format!(
                r#"<div class="dl dl-meta"><span class="ln"></span><span class="ln"></span><code>{}</code></div>"#,
                html_or_nbsp(raw)
            ));
            continue;
        }

        if let Some((old_start, new_start)) = hunk_starts(raw) {
            old_no = old_start;
            new_no = new_start;
            rows.push_str(&format!(
                r#"<div class="dl dl-hunk"><span class="ln"></span><span class="ln"></span><code>{}</code></div>"#,
                escape_html(raw)
            ));
            continue;
        }

        if raw.starts_with('+') && !raw.starts_with("+++") {
            rows.push_str(&format!(
                r#"<div class="dl dl-add"><span class="ln"></span><span class="ln">{}</span><code>{}</code></div>"#,
                new_no,
                html_or_nbsp(raw)
            ));
            new_no += 1;
        } else if raw.starts_with('-') && !raw.starts_with("---") {
            rows.push_str(&format!(
                r#"<div class="dl dl-del"><span class="ln">{}</span><span class="ln"></span><code>{}</code></div>"#,
                old_no,
                html_or_nbsp(raw)
            ));
            old_no += 1;
        } else {
            rows.push_str(&format!(
                r#"<div class="dl dl-ctx"><span class="ln">{}</span><span class="ln">{}</span><code>{}</code></div>"#,
                old_no,
                new_no,
                html_or_nbsp(raw)
            ));
            old_no += 1;
            new_no += 1;
        }
    }

    rows
}

pub fn build_html(view: &View) -> String {
    let total_add: u32 = view.files.iter().map(|f| f.added).sum();
    let total_del: u32 = view.files.iter().map(|f| f.removed).sum();
    let commit_rows = commit_rows(view);
    let toc = toc(&view.files);
    let file_blocks = file_blocks(&view.files);

    format!(
        r##"<!DOCTYPE html>
<html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title} · {repo}</title>
<style>
  :root{{
    --bg:#0a0d13; --term:#0d1117; --bar:#161b22; --line:#21262d; --line2:#2d333b;
    --ink:#c9d1d9; --dim:#768390; --bright:#e6edf3;
    --green:#3fb950; --green-lt:#56d364; --red:#f85149; --red-lt:#ffa198;
    --cyan:#39c5cf; --blue:#58a6ff; --amber:#d29922; --violet:#bc8cff;
    --addbg:rgba(46,160,67,.14); --delbg:rgba(248,81,73,.14);
    --mono:ui-monospace,"Cascadia Code","JetBrains Mono","SF Mono","Fira Code",Menlo,Consolas,monospace;
    color-scheme:dark;
  }}
  *{{box-sizing:border-box}}
  html{{scroll-behavior:smooth}}
  body{{
    margin:0;font-family:var(--mono);font-size:13.5px;line-height:1.55;color:var(--ink);
    background:
      radial-gradient(50vw 40vw at 88% -6%, rgba(57,197,207,.10), transparent 60%),
      radial-gradient(46vw 36vw at 6% 4%, rgba(63,185,80,.08), transparent 60%),
      var(--bg);
    background-attachment:fixed;-webkit-font-smoothing:antialiased;
  }}
  /* faint CRT scanlines for atmosphere */
  body::before{{content:"";position:fixed;inset:0;pointer-events:none;z-index:50;opacity:.5;background:repeating-linear-gradient(rgba(255,255,255,.013) 0 1px, transparent 1px 3px)}}
  ::selection{{background:rgba(63,185,80,.3)}}
  .term{{flex:0 1 1000px;min-width:0;max-width:1000px;margin:30px auto 80px;background:var(--term);border:1px solid var(--line2);border-radius:11px;overflow:hidden;box-shadow:0 30px 90px -28px rgba(0,0,0,.9), 0 0 70px -40px var(--cyan)}}
  .titlebar{{display:flex;align-items:center;gap:12px;background:var(--bar);border-bottom:1px solid var(--line);padding:10px 15px}}
  .dots{{display:flex;gap:8px;flex:none}}
  .dots i{{width:12px;height:12px;border-radius:50%;display:block;box-shadow:inset 0 0 0 1px rgba(0,0,0,.25)}}
  .dots i:nth-child(1){{background:#ff5f56}} .dots i:nth-child(2){{background:#ffbd2e}} .dots i:nth-child(3){{background:#27c93f}}
  .ttl{{flex:1;text-align:center;color:var(--dim);font-size:12px;letter-spacing:.04em;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}}
  .ttl b{{color:var(--bright);font-weight:600}}
  .screen{{padding:18px 20px 22px}}
  .cmd{{color:var(--dim);margin:2px 0;word-break:break-all}}
  .cmd .sig{{color:var(--green);font-weight:700;margin-right:8px}}
  .cmd .hl{{color:var(--cyan)}}
  .refline{{margin:8px 0 16px;display:flex;align-items:center;gap:9px;flex-wrap:wrap}}
  .ref{{border:1px solid var(--line2);border-radius:6px;padding:2px 9px;font-size:12.5px}}
  .ref-branch{{color:var(--amber)}} .ref-up{{color:var(--dim)}} .arr{{color:var(--dim)}}
  .stats{{display:flex;gap:8px;flex-wrap:wrap;margin-bottom:8px}}
  .stat{{border:1px solid var(--line2);border-radius:6px;padding:3px 11px;font-size:12px;color:var(--dim)}}
  .stat b{{color:var(--bright);font-weight:700}}
  .stat.add{{color:var(--green);border-color:rgba(63,185,80,.4)}} .stat.del{{color:var(--red);border-color:rgba(248,81,73,.4)}}
  .sec{{color:var(--green);opacity:.7;font-size:11.5px;letter-spacing:.06em;margin:24px 0 10px;border-bottom:1px solid var(--line);padding-bottom:7px}}
  .commit{{border-left:2px solid var(--line2);padding:4px 0 4px 13px;margin:2px 0;transition:border-color .12s;cursor:pointer}}
  .commit:hover{{border-left-color:var(--cyan)}} .commit:focus-visible{{outline:1px solid var(--cyan);outline-offset:2px;border-radius:3px}}
  .commit.active{{border-left-color:var(--amber);background:rgba(210,153,34,.08)}} .commit.active .sha{{text-decoration:underline}}
  .commit .crow{{display:flex;align-items:baseline;gap:10px}}
  .commit .sha{{color:var(--amber);background:none;border:0;padding:0;font-size:13px}}
  .commit .subj{{color:var(--ink);flex:1 1 auto;min-width:0;overflow-wrap:anywhere}}
  .commit .cdate{{flex:none;margin-left:auto;color:var(--dim);font-size:11.5px;font-variant-numeric:tabular-nums;letter-spacing:.02em;white-space:nowrap}}
  .commit .cbody{{color:var(--dim);font-size:12px;line-height:1.5;white-space:pre-wrap;border-left:2px solid var(--line);margin:6px 0 8px;padding:2px 0 2px 11px}}
  nav.toc{{display:flex;gap:7px;flex-wrap:wrap;margin:2px 0 14px}}
  nav.toc a{{color:var(--dim);text-decoration:none;border:1px solid var(--line2);border-radius:6px;padding:3px 10px;font-size:12px;white-space:nowrap;transition:border-color .12s,color .12s}}
  nav.toc a:hover{{border-color:var(--cyan);color:var(--bright)}} .a{{color:var(--green)}} .d{{color:var(--red)}}
  details.file{{border:1px solid var(--line);border-radius:8px;margin:10px 0;overflow:hidden;background:var(--term)}}
  details.file>summary{{cursor:pointer;list-style:none;display:flex;align-items:center;justify-content:space-between;gap:12px;padding:9px 13px;font-size:12.5px;background:var(--bar);position:sticky;top:0;z-index:5}}
  details.file>summary::-webkit-details-marker{{display:none}} details.file>summary::before{{content:"\25B8";color:var(--dim);transition:transform .15s;flex:none;font-size:11px}}
  details.file[open]>summary::before{{transform:rotate(90deg)}} details.file>summary:hover{{background:var(--line)}}
  details.file>summary .path{{flex:1;min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;color:var(--bright)}} .filestat{{flex:none;font-size:12px}}
  .diff{{border-top:1px solid var(--line);overflow-x:auto;font-size:12.5px;line-height:1.5}}
  .dl{{display:grid;grid-template-columns:46px 46px 1fr;align-items:start}}
  .dl .ln{{user-select:none;text-align:right;padding:0 8px;color:var(--dim);opacity:.5;font-size:11px;border-right:1px solid var(--line);white-space:nowrap;font-variant-numeric:tabular-nums}}
  .dl code{{padding:0 12px;white-space:pre;background:none;border:0;font-size:12.5px;color:#adbac7}}
  .dl-add{{background:var(--addbg);box-shadow:inset 2px 0 0 var(--green)}} .dl-add code{{color:var(--green-lt)}}
  .dl-del{{background:var(--delbg);box-shadow:inset 2px 0 0 var(--red)}} .dl-del code{{color:var(--red-lt)}}
  .dl-hunk{{background:rgba(57,197,207,.08)}} .dl-hunk code{{color:var(--cyan);font-weight:600}}
  .dl-meta{{opacity:.5}} .dl-meta code{{color:var(--dim)}}
  .empty{{color:var(--dim);font-style:italic;padding:16px;text-align:center;border:1px dashed var(--line2);border-radius:8px}}
  .foot{{margin-top:24px;border-top:1px solid var(--line);padding-top:15px;color:var(--dim)}}
  .cursor{{display:inline-block;width:8px;height:14px;background:var(--green);vertical-align:-2px;margin:0 2px;animation:blink 1.1s steps(1) infinite}}
  @keyframes blink{{50%{{opacity:0}}}} @media (prefers-reduced-motion:reduce){{.cursor{{animation:none}}}}
  .diff::-webkit-scrollbar{{height:10px}}
  .diff::-webkit-scrollbar-track{{background:var(--term)}}
  .diff::-webkit-scrollbar-thumb{{background:var(--line2);border-radius:6px;border:2px solid var(--term)}}
  /* === layout: terminal + optional file-tree sidebar (collapsed by default) === */
  .layout{{display:flex;align-items:flex-start;justify-content:center}} .layout[data-tree="open"] .term{{flex:1 1 auto;max-width:none;margin:30px 26px 80px}}
  .tree{{position:sticky;top:0;align-self:stretch;flex:0 0 0;width:0;height:100vh;overflow:hidden;background:var(--bar);border-right:1px solid var(--line);transition:flex-basis .2s ease,width .2s ease}}
  .layout[data-tree="open"] .tree{{flex:0 0 270px;width:270px}} .tree-head{{display:flex;align-items:center;justify-content:space-between;gap:8px;padding:13px 14px 10px;color:var(--dim);font-size:11px;letter-spacing:.12em;border-bottom:1px solid var(--line)}}
  .tree-body{{height:calc(100vh - 40px);overflow:auto;padding:8px 6px 40px;font-size:12.5px;white-space:nowrap}} .tree-body ul{{list-style:none;margin:0;padding-left:13px}} .tree-body>ul{{padding-left:3px}}
  .tlabel{{display:flex;align-items:center;gap:7px;padding:2px 6px;border-radius:5px;cursor:pointer;color:var(--ink)}} .tlabel:hover{{background:rgba(255,255,255,.05)}} .tdir>.tlabel .tname{{color:var(--blue)}}
  .tcaret{{flex:none;width:0;height:0;border-left:5px solid var(--dim);border-top:4px solid transparent;border-bottom:4px solid transparent;transition:transform .12s}} .tdir.open>.tlabel .tcaret{{transform:rotate(90deg)}} .tdir:not(.open)>ul{{display:none}}
  .tfile .ticon{{flex:none;width:6px;height:6px;border-radius:1px;background:var(--dim);opacity:.55}} .tname{{overflow:hidden;text-overflow:ellipsis}}
  .tree-body::-webkit-scrollbar{{width:9px}}
  .tree-body::-webkit-scrollbar-thumb{{background:var(--line2);border-radius:6px;border:2px solid var(--bar)}}
  .tree-toggle{{flex:none;display:flex;flex-direction:column;justify-content:center;gap:3px;width:28px;height:22px;padding:0 6px;background:transparent;border:1px solid var(--line2);border-radius:6px;cursor:pointer}}
  .tree-toggle span{{display:block;height:2px;background:var(--dim);border-radius:1px;transition:background .12s}} .tree-toggle:hover{{border-color:var(--cyan)}} .tree-toggle:hover span{{background:var(--bright)}} .tree-toggle[aria-expanded="true"]{{border-color:var(--cyan)}} .tree-toggle[aria-expanded="true"] span{{background:var(--cyan)}}
  .sec-commits{{display:flex;align-items:center;justify-content:space-between;gap:12px}} .showall{{font-family:var(--mono);font-size:11px;color:var(--amber);background:transparent;letter-spacing:0;border:1px solid rgba(210,153,34,.4);border-radius:5px;padding:2px 9px;cursor:pointer}}
  .showall:hover{{border-color:var(--amber);color:var(--bright)}}
  details.file.flash{{outline:1px solid var(--cyan);outline-offset:-1px}} details.file[hidden],nav.toc a[hidden]{{display:none}}
  @media print{{
    body{{background:#fff;color:#111}}body::before{{display:none}}
    .layout{{display:block}}.tree,.tree-toggle,.showall{{display:none!important}}
    .term{{box-shadow:none;border-color:#bbb;max-width:none;margin:0}}
    .titlebar{{background:#f2f2f2}}.ttl,.ttl b{{color:#111}}
    .cmd,.dim,.ref-up,.arr{{color:#555}}.commit .subj,.stat b,details.file>summary .path{{color:#000}}
    details.file{{break-inside:avoid}}details.file[hidden]{{display:block}}
    .dl code{{color:#111}}.cursor{{display:none}}
    .commit .cdate{{color:#555}}details.file>summary{{position:static;background:#f2f2f2}}
  }}
</style></head>
<body>
<div class="layout" data-tree="closed">
  <aside class="tree" aria-label="Changed files tree"><div class="tree-head"><span>FILES</span></div><div class="tree-body"></div></aside>
  <div class="term">
  <div class="titlebar">
    <button type="button" class="tree-toggle" aria-label="Toggle file tree" aria-expanded="false" title="Toggle file tree"><span></span><span></span><span></span></button>
    <span class="dots"><i></i><i></i><i></i></span>
    <span class="ttl">~/<b>{repo}</b> — {title}</span>
  </div>
  <div class="screen">
    <div class="cmd"><span class="sig">$</span>{cmd_lead}<span class="hl">{cmd_range}</span>{cmd_trail}</div>
    <div class="refline"><span class="ref ref-branch">{branch}</span><span class="arr">→</span><span class="ref ref-up">{upstream}</span></div>
    <div class="stats"><span class="stat"><b>{commit_count}</b> commit{commit_plural}</span><span class="stat"><b>{file_count}</b> file{file_plural}</span><span class="stat add">+{total_add}</span><span class="stat del">−{total_del}</span></div>
    <div class="sec sec-commits"><span>{commits_label}</span><button type="button" class="showall" hidden>show all ✕</button></div>
    {commit_rows}
    <div class="sec"># diff</div>
    {toc}
    {file_blocks}
    <div class="cmd foot"><span class="sig">$</span>{foot_cmd}<span class="cursor"></span> <span class="dim">{foot_note}</span></div>
  </div>
  </div>
</div>
<script>
(function(){{
  var layout = document.querySelector('.layout');
  var fileEls = [].slice.call(document.querySelectorAll('details.file'));
  var tocLinks = [].slice.call(document.querySelectorAll('nav.toc a'));
  var commitEls = [].slice.call(document.querySelectorAll('.commit[data-sha]'));
  var showAll = document.querySelector('.showall');
  var treeToggle = document.querySelector('.tree-toggle');
  var treeBody = document.querySelector('.tree-body');
  var activeSha = null;
  function esc(s){{ return String(s).replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;'); }}
  function shasOf(el){{ return (el.getAttribute('data-commits')||'').split(' ').filter(Boolean); }}
  // ---- commit filter: show only the files a commit touched ----
  function applyFilter(){{
    fileEls.forEach(function(el){{ el.hidden = !!activeSha && shasOf(el).indexOf(activeSha) === -1; }});
    tocLinks.forEach(function(a){{ a.hidden = !!activeSha && shasOf(a).indexOf(activeSha) === -1; }});
    commitEls.forEach(function(c){{ c.classList.toggle('active', activeSha === c.getAttribute('data-sha')); }});
    if (showAll) showAll.hidden = !activeSha;
    buildTree();
  }}
  commitEls.forEach(function(c){{
    function toggle(){{ var s = c.getAttribute('data-sha'); activeSha = (activeSha === s) ? null : s; applyFilter(); }}
    c.addEventListener('click', toggle);
    c.addEventListener('keydown', function(e){{ if (e.key === 'Enter' || e.key === ' ') {{ e.preventDefault(); toggle(); }} }});
  }});
  if (showAll) showAll.addEventListener('click', function(){{ activeSha = null; applyFilter(); }});
  // ---- sidebar toggle: flips the layout state attribute; CSS reflows the rest ----
  if (treeToggle) treeToggle.addEventListener('click', function(){{
    var open = layout.getAttribute('data-tree') === 'open';
    layout.setAttribute('data-tree', open ? 'closed' : 'open');
    treeToggle.setAttribute('aria-expanded', String(!open));
  }});
  // ---- file tree: fold visible file paths into a nested, collapsible tree ----
  function buildTree(){{
    if (!treeBody) return;
    var root = {{ dirs:{{}}, files:[] }};
    fileEls.forEach(function(el){{
      if (el.hidden) return;
      var parts = (el.getAttribute('data-path')||'').split('/');
      var node = root;
      for (var i = 0; i < parts.length - 1; i++){{ node = node.dirs[parts[i]] = node.dirs[parts[i]] || {{ dirs:{{}}, files:[] }}; }}
      node.files.push({{ name: parts[parts.length - 1], id: el.id }});
    }});
    treeBody.innerHTML = '';
    treeBody.appendChild(renderNode(root));
  }}
  function renderNode(node){{
    var ul = document.createElement('ul');
    Object.keys(node.dirs).sort().forEach(function(name){{
      var li = document.createElement('li'); li.className = 'tnode tdir open';
      var label = document.createElement('div'); label.className = 'tlabel';
      label.innerHTML = '<span class="tcaret"></span><span class="tname">' + esc(name) + '</span>';
      label.addEventListener('click', function(){{ li.classList.toggle('open'); }});
      li.appendChild(label); li.appendChild(renderNode(node.dirs[name])); ul.appendChild(li);
    }});
    node.files.sort(function(a,b){{ return a.name < b.name ? -1 : 1; }}).forEach(function(f){{
      var li = document.createElement('li'); li.className = 'tnode tfile';
      var label = document.createElement('div'); label.className = 'tlabel';
      label.innerHTML = '<span class="ticon"></span><span class="tname">' + esc(f.name) + '</span>';
      label.addEventListener('click', function(){{
        var t = document.getElementById(f.id);
        if (!t) return;
        t.open = true; t.scrollIntoView({{ behavior:'smooth', block:'start' }});
        t.classList.add('flash'); setTimeout(function(){{ t.classList.remove('flash'); }}, 1200);
      }});
      li.appendChild(label); ul.appendChild(li);
    }});
    return ul;
  }}
  buildTree();
}})();
</script>
</body></html>"##,
        title = escape_html(&view.title),
        repo = escape_html(&view.repo_name),
        cmd_lead = escape_html(&view.cmd.lead),
        cmd_range = escape_html(&view.cmd.range),
        cmd_trail = escape_html(&view.cmd.trail),
        branch = escape_html(&view.branch),
        upstream = escape_html(&view.upstream),
        commit_count = view.commits.len(),
        commit_plural = plural(view.commits.len()),
        file_count = view.files.len(),
        file_plural = plural(view.files.len()),
        total_add = total_add,
        total_del = total_del,
        commits_label = escape_html(&view.commits_label),
        commit_rows = commit_rows,
        toc = toc,
        file_blocks = file_blocks,
        foot_cmd = escape_html(&view.foot.cmd),
        foot_note = escape_html(&view.foot.note),
    )
}

fn is_meta_line(raw: &str) -> bool {
    raw.starts_with("index ")
        || raw.starts_with("--- ")
        || raw.starts_with("+++ ")
        || raw.starts_with("new file")
        || raw.starts_with("deleted file")
        || raw.starts_with("old mode")
        || raw.starts_with("new mode")
        || raw.starts_with("similarity ")
        || raw.starts_with("rename ")
        || raw.starts_with("Binary ")
        || raw.starts_with('\\')
}

fn html_or_nbsp(raw: &str) -> String {
    let escaped = escape_html(raw);
    if escaped.is_empty() {
        "&nbsp;".to_string()
    } else {
        escaped
    }
}

fn hunk_starts(raw: &str) -> Option<(u32, u32)> {
    let rest = raw.strip_prefix("@@ -")?;
    let (old_part, rest) = rest.split_once(" +")?;
    let (new_part, _) = rest.split_once(" @@")?;
    Some((parse_hunk_range(old_part)?, parse_hunk_range(new_part)?))
}

fn parse_hunk_range(s: &str) -> Option<u32> {
    let (start, len) = match s.split_once(',') {
        Some((start, len)) => (start, Some(len)),
        None => (s, None),
    };

    if start.is_empty()
        || !start.chars().all(|ch| ch.is_ascii_digit())
        || len.is_some_and(|value| value.is_empty() || !value.chars().all(|ch| ch.is_ascii_digit()))
    {
        return None;
    }

    start.parse().ok()
}

fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}

fn file_commits(file: &FileDiff) -> String {
    file.commits.join(" ")
}

fn commit_rows(view: &View) -> String {
    if view.commits.is_empty() {
        return r#"<div class="empty">no commits in range</div>"#.to_string();
    }

    view.commits
        .iter()
        .map(|commit| {
            let time = if commit.date.is_empty() {
                String::new()
            } else if commit.iso.is_empty() {
                format!(
                    r#"<time class="cdate">{}</time>"#,
                    escape_html(&commit.date)
                )
            } else {
                format!(
                    r#"<time class="cdate" datetime="{}" title="{}">{}</time>"#,
                    escape_html(&commit.iso),
                    escape_html(&commit.iso),
                    escape_html(&commit.date)
                )
            };
            let body = if commit.body.trim().is_empty() {
                String::new()
            } else {
                format!(
                    r#"<pre class="cbody">{}</pre>"#,
                    escape_html(commit.body.trim())
                )
            };
            format!(
                r#"<div class="commit" data-sha="{}" role="button" tabindex="0" title="show only this commit's files"><div class="crow"><code class="sha">{}</code><span class="subj">{}</span>{}</div>{}</div>"#,
                escape_html(&commit.sha),
                escape_html(&commit.sha),
                escape_html(&commit.subject),
                time,
                body
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn toc(files: &[FileDiff]) -> String {
    if files.is_empty() {
        return String::new();
    }

    let links = files
        .iter()
        .map(|file| {
            let name = file.path.rsplit('/').next().unwrap_or(&file.path);
            format!(
                r##"<a href="#{}" data-commits="{}">{} <span class="a">+{}</span> <span class="d">−{}</span></a>"##,
                slug(&file.path),
                escape_html(&file_commits(file)),
                escape_html(name),
                file.added,
                file.removed
            )
        })
        .collect::<String>();

    format!(r#"<nav class="toc" aria-label="Changed files">{links}</nav>"#)
}

fn file_blocks(files: &[FileDiff]) -> String {
    if files.is_empty() {
        return r#"<div class="empty">no file changes</div>"#.to_string();
    }

    files
        .iter()
        .map(|file| {
            format!(
                r#"<details open id="{}" class="file" data-path="{}" data-commits="{}"><summary><span class="path">{}</span><span class="filestat"><span class="a">+{}</span> <span class="d">−{}</span></span></summary><div class="diff">{}</div></details>"#,
                slug(&file.path),
                escape_html(&file.path),
                escape_html(&file_commits(file)),
                escape_html(&file.path),
                file.added,
                file.removed,
                render_diff_lines(&file.lines)
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Cmd, Commit, FileDiff, Foot, View};

    #[test]
    fn escape_html_escapes_metacharacters() {
        assert_eq!(escape_html("&<>\""), "&amp;&lt;&gt;&quot;");
    }

    #[test]
    fn slug_normalizes_file_paths_to_anchor_ids() {
        assert_eq!(slug("src/a b.rs"), "f-src-a-b-rs");
    }

    #[test]
    fn slug_preserves_prefix_when_normalized_body_is_empty() {
        assert_eq!(slug("---"), "f-");
    }

    #[test]
    fn render_diff_lines_classifies_rows_and_tracks_gutter_numbers() {
        let html = render_diff_lines(&[
            "index 111..222 100644".to_string(),
            "@@ -3,2 +7,2 @@".to_string(),
            " keep".to_string(),
            "-old".to_string(),
            "+new".to_string(),
        ]);

        assert!(html.contains(r#"<div class="dl dl-meta"><span class="ln"></span><span class="ln"></span><code>index 111..222 100644</code></div>"#));
        assert!(html.contains(r#"<div class="dl dl-hunk"><span class="ln"></span><span class="ln"></span><code>@@ -3,2 +7,2 @@</code></div>"#));
        assert!(html.contains(r#"<div class="dl dl-ctx"><span class="ln">3</span><span class="ln">7</span><code> keep</code></div>"#));
        assert!(html.contains(r#"<div class="dl dl-del"><span class="ln">4</span><span class="ln"></span><code>-old</code></div>"#));
        assert!(html.contains(r#"<div class="dl dl-add"><span class="ln"></span><span class="ln">8</span><code>+new</code></div>"#));
    }

    #[test]
    fn render_diff_lines_does_not_treat_malformed_headers_as_hunks() {
        let html = render_diff_lines(&["@@ -1, +2 @@".to_string()]);

        assert!(!html.contains("dl-hunk"));
        assert!(html.contains(r#"<div class="dl dl-ctx"><span class="ln">0</span><span class="ln">0</span><code>@@ -1, +2 @@</code></div>"#));
    }

    #[test]
    fn build_html_renders_offline_document_with_core_diff_data() {
        let view = View {
            repo_name: "api".to_string(),
            branch: "main".to_string(),
            upstream: "origin/main".to_string(),
            commits: vec![Commit {
                sha: "abc123def".to_string(),
                subject: "feat: thing".to_string(),
                body: String::new(),
                date: String::new(),
                iso: String::new(),
            }],
            files: vec![FileDiff {
                path: "src/a b.rs".to_string(),
                added: 2,
                removed: 1,
                lines: vec![
                    "@@ -1 +1,2 @@".to_string(),
                    "-old".to_string(),
                    "+new".to_string(),
                    "+extra".to_string(),
                ],
                commits: vec!["abc123def".to_string()],
            }],
            title: "diff".to_string(),
            cmd: Cmd {
                lead: "git diff ".to_string(),
                range: "origin/main..HEAD".to_string(),
                trail: String::new(),
            },
            commits_label: "# commits".to_string(),
            foot: Foot {
                cmd: "git diff origin/main..HEAD".to_string(),
                note: "# read-only preview".to_string(),
            },
        };

        let html = build_html(&view);

        assert!(html.starts_with("<!DOCTYPE html>"));
        assert!(!html.contains("http://"));
        assert!(!html.contains("https://"));
        assert!(html.contains("api"));
        assert!(html.contains("origin/main..HEAD"));
        assert!(html.contains("src/a b.rs"));
        assert!(html.contains(r#"<span class="a">+2</span>"#));
        assert!(html.contains(r#"<span class="d">−1</span>"#));
    }

    #[test]
    fn build_html_keeps_original_static_renderer_chrome() {
        let html = build_html(&sample_view());

        assert!(html.contains(
            "radial-gradient(50vw 40vw at 88% -6%, rgba(57,197,207,.10), transparent 60%)"
        ));
        assert!(html.contains("background-attachment:fixed;-webkit-font-smoothing:antialiased;"));
        assert!(
            html.contains(
                "box-shadow:0 30px 90px -28px rgba(0,0,0,.9), 0 0 70px -40px var(--cyan)"
            )
        );
        assert!(html.contains(".diff::-webkit-scrollbar{height:10px}"));
        assert!(html.contains(".showall:hover{border-color:var(--amber);color:var(--bright)}"));
        assert!(html.contains(".titlebar{background:#f2f2f2}.ttl,.ttl b{color:#111}"));
        assert!(html.contains("// ---- commit filter: show only the files a commit touched ----"));
        assert!(html.contains("t.scrollIntoView({ behavior:'smooth', block:'start' });"));
    }

    #[test]
    fn build_html_escapes_user_controlled_values() {
        let mut view = sample_view();
        view.repo_name = "a&b<repo>\"".to_string();
        view.branch = "main<script>".to_string();
        view.upstream = "origin/feat\"x".to_string();
        view.cmd.range = "HEAD~1..HEAD&bad".to_string();
        view.files[0].path = "src/<x>&\".rs".to_string();
        view.files[0].lines = vec![
            "@@ -0,0 +1 @@".to_string(),
            "+<script>x</script>".to_string(),
        ];

        let html = build_html(&view);

        assert!(html.contains("a&amp;b&lt;repo&gt;&quot;"));
        assert!(html.contains("main&lt;script&gt;"));
        assert!(html.contains("origin/feat&quot;x"));
        assert!(html.contains("HEAD~1..HEAD&amp;bad"));
        assert!(html.contains("src/&lt;x&gt;&amp;&quot;.rs"));
        assert!(!html.contains("<script>x</script>"));
        assert!(html.contains("+&lt;script&gt;x&lt;/script&gt;"));
    }

    fn sample_view() -> View {
        View {
            repo_name: "api".to_string(),
            branch: "main".to_string(),
            upstream: "origin/main".to_string(),
            commits: vec![Commit {
                sha: "abc123def".to_string(),
                subject: "feat: thing".to_string(),
                body: String::new(),
                date: String::new(),
                iso: String::new(),
            }],
            files: vec![FileDiff {
                path: "src/a b.rs".to_string(),
                added: 2,
                removed: 1,
                lines: vec![
                    "@@ -1 +1,2 @@".to_string(),
                    "-old".to_string(),
                    "+new".to_string(),
                    "+extra".to_string(),
                ],
                commits: vec!["abc123def".to_string()],
            }],
            title: "diff".to_string(),
            cmd: Cmd {
                lead: "git diff ".to_string(),
                range: "origin/main..HEAD".to_string(),
                trail: String::new(),
            },
            commits_label: "# commits".to_string(),
            foot: Foot {
                cmd: "git diff origin/main..HEAD".to_string(),
                note: "# read-only preview".to_string(),
            },
        }
    }
}
