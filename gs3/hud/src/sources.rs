mod screenshot;
mod stack;
mod voice;
mod media;
mod spotify;
pub use stack::stack;
pub use screenshot::screenshot;
pub use voice::voice;
pub use media::media;
pub use spotify::spotify;
use crate::core::*;
use anyhow::{anyhow, Context, Result};
use notify::{RecursiveMode, Watcher};
use rusqlite::{Connection, OpenFlags};
use serde_json::{json, Value};
use std::{fs, io::{BufRead, BufReader, Read, Seek, SeekFrom, Write}, os::unix::net::UnixStream, path::{Path, PathBuf}, process::{Command, Stdio}, sync::mpsc::{self, Sender}, sync::Mutex, thread, time::{Duration, Instant}};

// Drain both pipes concurrently: even a noisy subprocess cannot deadlock the timeout.
pub fn run(cwd:&Path,program:&str,args:&[&str],seconds:u64)->Result<String> {
    let mut child=Command::new(program).args(args).current_dir(cwd).env("GIT_OPTIONAL_LOCKS","0").stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;
    fn drain<R:Read+Send+'static>(mut pipe:R)->thread::JoinHandle<String> {thread::spawn(move||{let mut s=String::new(); let _=pipe.read_to_string(&mut s); s})}
    let out=drain(child.stdout.take().unwrap()); let err=drain(child.stderr.take().unwrap());
    let deadline=Instant::now()+Duration::from_secs(seconds);
    let status=loop {if let Some(s)=child.try_wait()? {break s} if Instant::now()>=deadline {let _=child.kill(); let _=child.wait(); return Err(anyhow!("{program}: timeout ({seconds}s)"))} thread::sleep(Duration::from_millis(20));};
    let stdout=out.join().unwrap_or_default(); let stderr=err.join().unwrap_or_default();
    if status.success() {Ok(stdout.trim_end().into())} else {Err(anyhow!("{}: {}",program,stderr.trim()))}
}
pub fn git(cwd:&Path,args:&[&str])->Result<String> {run(cwd,"git",args,3)}
pub fn git_root(cwd:&Path)->Option<PathBuf> {git(cwd,&["rev-parse","--show-toplevel"]).ok().map(PathBuf::from)}
pub fn git_dir(root:&Path)->PathBuf {
    let p=root.join(".git");
    if p.is_file() {gitdir_target(root,&fs::read_to_string(&p).unwrap_or_default())} else {p}
}
/// One page of the graph, and the ceiling: the rail is a rail, not a log viewer.
pub const LOG_PAGE:usize=30; pub const LOG_MAX:usize=300;
pub fn git_snapshot(cwd:&Path)->Git { git_log(cwd,LOG_PAGE) }
/// The same snapshot with the graph asking for `limit` commits: the extra pages only change that number.
pub fn git_log(cwd:&Path,limit:usize)->Git {
    let limit=limit.clamp(1,LOG_MAX);
    let Some(root)=git_root(cwd) else {return Git{error:Some("not a git repo".into()),..Git::default()}};
    let head=fs::read_to_string(git_dir(&root).join("HEAD")).unwrap_or_default();
    let mut g=Git{branch:head_name(&head),log_limit:limit,..Git::default()}; let mut errors=Vec::new();
    // Porcelain paths are relative to the repository root: every command runs there.
    let cwd=root.as_path();
    match git(cwd,&["status","--porcelain=v1","-z","--no-renames","--untracked-files=all"]) {Ok(s)=>{let (files,counts)=parse_status(&s); g.dirty=Some(files.len()); g.files=files; g.counts=counts;},Err(e)=>errors.push(e.to_string())}
    // No commits yet: there is no HEAD to diff against.
    if let Ok(s)=git(cwd,&["diff","HEAD","--numstat","-z","--no-renames"]) {(g.added,g.deleted)=apply_numstat(&mut g.files,&s);}
    if let Ok(s)=git(cwd,&["rev-list","--left-right","--count","@{u}...HEAD"]) {let n:Vec<u64>=s.split_whitespace().filter_map(|x|x.parse().ok()).collect(); if n.len()==2 {g.ahead_behind=Some((n[1],n[0]));}}
    let n=limit.to_string();
    match git(cwd,&["log","--graph","--oneline","--decorate=short","--color=never","-n",&n]) {
        Ok(s)=>{g.commits=graph_commits(&s); g.graph=s.lines().map(str::to_owned).collect();}
        Err(e) if e.to_string().contains("does not have any commits")=>{},
        Err(e)=>errors.push(e.to_string())
    }
    if let Ok(s)=git(cwd,&["for-each-ref","--format=%(refname)%09%(refname:short)%09%(upstream:short)%09%(upstream:track)%09%(committerdate:relative)","refs/heads","refs/remotes"]) {g.branches=parse_branches(&s,&g.branch);}
    if !errors.is_empty() {g.error=Some(errors.join("; "));}
    g
}
/// Names the TREE tab never lists (reference `DEFAULT_IGNORED_NAMES` + Rust `target`).
const TREE_SKIP:[&str;9]=[".git","node_modules","dist",".cache",".next","coverage",".turbo","build","target"];
const TREE_MAX:usize=2000;
/// Scan `cwd`'s git root (or `cwd`): dirs first, then files, both sorted case-insensitively. Only directories in
/// `open` (whose parents are open too) are read further; the others only count their entries. Symlinks are listed
/// as files and never followed. Names only: no file is read.
pub fn tree(cwd:&Path,open:Vec<String>)->Tree {
    let root=git_root(cwd).unwrap_or_else(||cwd.to_owned());
    let project=root.file_name().unwrap_or_default().to_string_lossy().into_owned();
    fn entries(dir:&Path)->std::io::Result<Vec<(String,bool)>> {
        let mut v:Vec<(String,bool)>=fs::read_dir(dir)?.flatten().filter_map(|e|{let name=e.file_name().to_string_lossy().into_owned();
            (!TREE_SKIP.contains(&name.as_str())).then(||(name,e.file_type().is_ok_and(|t|t.is_dir())))}).collect();
        v.sort_by(|a,b|b.1.cmp(&a.1).then_with(||a.0.to_lowercase().cmp(&b.0.to_lowercase()))); Ok(v)
    }
    fn scan(dir:&Path,rel:&str,open:&[String],budget:&mut usize)->std::io::Result<Vec<TreeNode>> {
        let mut out=Vec::new();
        for (name,is_dir) in entries(dir)? {
            if *budget==0 {break} *budget-=1;
            let r=if rel.is_empty() {name.clone()} else {format!("{rel}/{name}")};
            let (count,children)=if !is_dir {(0,Vec::new())} else if open.contains(&r) {
                let c=scan(&dir.join(&name),&r,open,budget).unwrap_or_default(); (c.len(),c)
            } else {(entries(&dir.join(&name)).map(|e|e.len()).unwrap_or(0),Vec::new())};
            out.push(TreeNode{name,rel:r,dir:is_dir,count,children});
        }
        Ok(out)
    }
    let mut budget=TREE_MAX;
    match scan(&root,"",&open,&mut budget) {Ok(nodes)=>Tree{project,open,nodes,error:None},Err(e)=>Tree{project,open,nodes:Vec::new(),error:Some(e.to_string())}}
}
/// TREE scan worker: answers only the newest open set.
pub fn trees(cwd:PathBuf,tx:Sender<Event>)->Sender<Vec<String>> {
    let (send,rx)=mpsc::channel::<Vec<String>>();
    thread::spawn(move||{while let Ok(mut open)=rx.recv() {
        while let Ok(n)=rx.try_recv() {open=n;}
        if tx.send(Event::Tree(tree(&cwd,open))).is_err() {break}
    }}); send
}
/// Extensions the window draws as an image instead of reading as text.
const IMAGE_EXT:[&str;10]=["png","jpg","jpeg","gif","webp","bmp","tif","tiff","ico","heic"];
/// Extensions the window renders as a document instead of drawing the bytes as raw text.
const MARKDOWN_EXT:[&str;2]=["md","markdown"];
/// Ceiling for a previewed text file: past it the window shows the head and says how much it left out.
pub const PREVIEW_MAX:u64=256*1024;
/// Ceiling for an image: the window decodes it, so a file that big is refused instead of read into memory.
const IMAGE_MAX:u64=32*1024*1024;
/// A file is data, not instructions: an ESC in its bytes would be read as colour by the window's SGR parser,
/// so it is shown as the symbol it is (U+241B) instead. Everything the window renders as SGR goes through here.
pub fn neutral(text:&str)->String {
    if !text.contains('\u{1b}') {return text.to_owned()}
    text.replace('\u{1b}', "\u{241b}")
}
/// What a TREE click shows in the rail's own window: the file as text, the image itself, or why neither is
/// allowed. The gate is the same one `detail` already applies: nothing outside the project, no symlink, no
/// sensitive name, no binary — plus a size ceiling, because this reads the file instead of a diff.
/// An image is never read here: its path travels and the window draws it.
pub fn preview(root:&Path,rel:&str)->Preview {
    let refused=|note:&str|Preview::Refused{path:rel.into(),note:note.into()};
    if rel.is_empty()||rel.starts_with('/')||rel.split('/').any(|p|p.is_empty()||p=="..") {return refused("fuera del proyecto")}
    if sensitive(rel) {return refused("archivo sensible (no se muestra)")}
    let file=root.join(rel);
    match fs::symlink_metadata(&file) {
        Err(e)=>return refused(&format!("no se pudo leer: {e}")),
        Ok(m) if m.file_type().is_symlink()=>return refused("enlace simbólico (no se sigue)"),
        Ok(m) if m.is_dir()=>return refused("directorio, no archivo"),
        Ok(_)=>{}
    }
    let ext=rel.rsplit_once('.').map(|(_,e)|e.to_ascii_lowercase()).unwrap_or_default();
    if IMAGE_EXT.contains(&ext.as_str()) {
        let len=fs::metadata(&file).map(|m|m.len()).unwrap_or(0);
        return if len>IMAGE_MAX {refused(&format!("imagen de {} MiB (demasiado grande)",len/1024/1024))} else {Preview::Image{path:rel.into(),file}}
    }
    match fs::read(&file) {
        Err(e)=>refused(&format!("no se pudo leer: {e}")),
        Ok(b) if b.contains(&0)=>refused("archivo binario (no se muestra)"),
        Ok(mut b)=>{let cut=b.len() as u64>PREVIEW_MAX; b.truncate(PREVIEW_MAX as usize);
            let body=neutral(&String::from_utf8_lossy(&b));
            if MARKDOWN_EXT.contains(&ext.as_str()) {Preview::Markdown{path:rel.into(),body,cut}}
            // A file its grammar can colour is a source file: the window draws the colours, not the bytes.
            else if let Some(lang)=crate::syntax::language(rel,&body) {Preview::Code{path:rel.into(),body,cut,lang}}
            else {Preview::Text{path:rel.into(),body,cut}}}
    }
}
/// TREE preview worker: reads off the draw loop and answers only the newest click.
pub fn previews(cwd:PathBuf,tx:Sender<Event>)->Sender<String> {
    let (send,rx)=mpsc::channel::<String>();
    thread::spawn(move||{
        let root=git_root(&cwd).unwrap_or(cwd);
        while let Ok(mut rel)=rx.recv() {
            while let Ok(n)=rx.try_recv() {rel=n;}
            if tx.send(Event::Preview(preview(&root,&rel))).is_err() {break}
        }
    }); send
}
/// What the GIT tab inspects: a branch diffed against HEAD and/or one file (untracked files are read directly).
#[derive(Clone, Debug, PartialEq)]
pub struct DetailRequest { pub branch:Option<String>, pub file:Option<String>, pub commit:Option<String>, pub untracked:bool }
const DIFF_KEEP:usize=2000; const UNTRACKED_MAX:u64=64*1024;
fn diff_lines(d:&mut Detail,text:&str) {let text=neutral(text); let all:Vec<&str>=text.lines().collect(); d.more=all.len().saturating_sub(DIFF_KEEP); d.diff=all.into_iter().take(DIFF_KEEP).map(str::to_owned).collect();}
pub fn detail(cwd:&Path,r:&DetailRequest)->Detail {
    let mut d=Detail{branch:r.branch.clone(),file:r.file.clone(),commit:r.commit.clone(),..Detail::default()};
    let Some(root)=git_root(cwd) else {d.error=Some("not a git repo".into()); return d};
    if let Some(b)=&r.branch {
        let range=format!("HEAD...{b}");
        match git(&root,&["diff","--name-status","-z","--no-renames","--end-of-options",&range]) {
            Ok(s)=>{d.files=parse_name_status(&s); if let Ok(n)=git(&root,&["diff","--numstat","-z","--no-renames","--end-of-options",&range]) {(d.added,d.deleted)=apply_numstat(&mut d.files,&n);}}
            Err(e)=>d.error=Some(e.to_string()),
        }
    }
    if let Some(c)=r.commit.as_deref().filter(|c|crate::core::is_hash(c)) {
        d.commit=Some(c.to_string());
        // One subprocess for the whole message, never one per hover or per click on a file.
        if let Ok(s)=git(&root,&["show","-s","--format=%an%x00%ar%x00%B",c]) {
            let mut parts=s.splitn(3,'\0');
            d.author=parts.next().unwrap_or_default().trim().to_string();
            d.date=parts.next().unwrap_or_default().trim().to_string();
            d.message=neutral(parts.next().unwrap_or_default().trim_end());
        }
        match git(&root,&["show","--name-status","-z","--no-renames","--format=",c]) {
            Ok(s)=>{d.files=parse_name_status(&s); if let Ok(n)=git(&root,&["show","--numstat","-z","--no-renames","--format=",c]) {(d.added,d.deleted)=apply_numstat(&mut d.files,&n);}}
            Err(e)=>d.error=Some(e.to_string()),
        }
    } else if r.commit.is_some() { d.error=Some("hash inválido".into()); }
    let Some(f)=&r.file else {return d};
    // Never read nor diff what may hold credentials (the UI says so instead).
    if sensitive(f) {return d}
    let Some(c)=r.commit.as_deref().filter(|c|crate::core::is_hash(c)) else {
    let out=match &r.branch {
        Some(b)=>git(&root,&["diff","--no-renames","--end-of-options",&format!("HEAD...{b}"),"--",f]),
        None if r.untracked=>{
            let p=root.join(f);
            // Symlinks could point outside the repository: not followed.
            match fs::symlink_metadata(&p) {
                Ok(m) if m.file_type().is_symlink()=>Ok("enlace simbólico (no se sigue)".into()),
                Ok(m) if m.is_dir()=>Ok("directorio sin seguimiento".into()),
                Ok(m) if m.len()>UNTRACKED_MAX=>Ok(format!("archivo nuevo de {} KiB (no se muestra)",m.len()/1024)),
                Ok(_)=>fs::read(&p).map_err(Into::into).map(|b|if b.contains(&0) {"archivo binario".into()} else {
                    let t=String::from_utf8_lossy(&b); format!("@@ archivo nuevo @@\n{}",t.lines().map(|l|format!("+{l}")).collect::<Vec<_>>().join("\n"))}),
                Err(e)=>Err(e.into()),
            }
        }
        None=>git(&root,&["diff","HEAD","--no-renames","--",f]),
    };
    match out {Ok(s)=>diff_lines(&mut d,&s),Err(e)=>d.diff=vec![format!("error: {e}")]}
    return d };
    match git(&root,&["show","--no-renames","--format=",c,"--",f]) {Ok(s)=>diff_lines(&mut d,&s),Err(e)=>d.diff=vec![format!("error: {e}")]}
    d
}
/// What the rail asks its own window to do. The window is another process, so it has to be owned: a
/// thread keeps the child and kills it when the selection moves or the rail goes away, exactly like the
/// video window's player.
pub enum PanelCmd { Show { hash: String, text: String, geometry: Option<String> }, Markdown { key: String, text: String, title: String, cut: bool }, Image { key: String, file: PathBuf }, Hide }
/// The panel binary: next to the rail first (the installer puts both in the same bin, and that directory is
/// not always on the PATH the pane inherits), then the PATH itself.
fn panel_bin()->Option<PathBuf> {
    std::env::current_exe().ok().and_then(|e|e.parent().map(|d|d.join("gs3-panel"))).filter(|p|p.is_file()).or_else(||media::find_tool("gs3-panel"))
}

pub fn panel(socket: PathBuf, agent_pane: String, events: Sender<Event>) -> Sender<PanelCmd> {
    let (send, rx) = mpsc::channel::<PanelCmd>();
    // One socket per rail process: the window says which file was clicked and nothing else, and the rail
    // turns that into the same action its own card would produce.
    let hud_socket = std::env::temp_dir().join(format!("gs3-panel-{}.sock", std::process::id()));
    let _ = fs::remove_file(&hud_socket);
    if let Ok(listener) = std::os::unix::net::UnixListener::bind(&hud_socket) {
        thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let mut line = String::new();
                if std::io::BufRead::read_line(&mut std::io::BufReader::new(stream), &mut line).is_ok() {
                    if let Some(path) = line.trim().strip_prefix("file ").filter(|p| !p.is_empty()) {
                        let _ = events.send(Event::Ui(crate::ui::Action::File(path.to_string())));
                    }
                }
            }
        });
    }
    thread::spawn(move || {
        let mut open: Option<(String, std::process::Child)> = None;
        let mut close = |open: &mut Option<(String, std::process::Child)>| {
            if let Some((_, mut child)) = open.take() { let _ = child.kill(); let _ = child.wait(); }
        };
        while let Ok(mut cmd) = rx.recv() {
            while let Ok(next) = rx.try_recv() { cmd = next; }
            match cmd {
                PanelCmd::Hide => close(&mut open),
                PanelCmd::Show { hash, text, geometry } => {
                    let path = std::env::temp_dir().join(format!("gs3-panel-{}.txt", std::process::id()));
                    // The same commit already on screen is left alone: rewriting its text is enough for the
                    // window, which follows the file, so clicking a file never moves or rebuilds it.
                    if open.as_ref().is_some_and(|(up, _)| *up == hash) { let _ = fs::write(&path, text); continue }
                    close(&mut open);
                    let Some(bin) = panel_bin() else { continue };
                    if fs::write(&path, text).is_err() { continue }
                    let mut args = geometry.map(|g| vec!["--geometry".to_string(), g]).unwrap_or_default();
                    args.push("--socket".to_string()); args.push(hud_socket.to_string_lossy().into_owned());
                    let spawned = std::process::Command::new(bin).args(args).arg(&path).spawn();
                    if let Ok(child) = spawned { open = Some((hash, child)); }
                }
                // A markdown file: the window renders it, because headings and tables as raw text are the
                // one thing a rail with 50 columns cannot show. Same window; it is just told what it is.
                PanelCmd::Markdown { key, text, title, cut } => {
                    let path = std::env::temp_dir().join(format!("gs3-panel-{}.md", std::process::id()));
                    if open.as_ref().is_some_and(|(up, _)| *up == key) { let _ = fs::write(&path, text); continue }
                    close(&mut open);
                    let Some(bin) = panel_bin() else { continue };
                    if fs::write(&path, text).is_err() { continue }
                    // `--title`: the file the window shows is the rail's temporary copy, but the name it must
                    // show is the project-relative one the human clicked.
                    let mut args = vec!["--socket".to_string(), hud_socket.to_string_lossy().into_owned(), "--markdown".to_string(), "--title".to_string(), title];
                    if cut { args.push("--cut".to_string()) }
                    let spawned = std::process::Command::new(bin).args(args).arg(&path).spawn();
                    if let Ok(child) = spawned { open = Some((key, child)); }
                }
                // A TREE image: the window reads and draws the file itself, so the bytes never enter the rail.
                PanelCmd::Image { key, file } => {
                    if open.as_ref().is_some_and(|(up, _)| *up == key) { continue }
                    close(&mut open);
                    let Some(bin) = panel_bin() else { continue };
                    let spawned = std::process::Command::new(bin)
                        .arg("--socket").arg(hud_socket.to_string_lossy().into_owned())
                        .arg("--image").arg(&file).spawn();
                    if let Ok(child) = spawned { open = Some((key, child)); }
                }
            }
        }
        close(&mut open);
        let _ = socket; let _ = agent_pane;
    });
    send
}

/// Inspector worker: only the latest pending request is answered (clicks may outrun git).
pub fn details(cwd:PathBuf,tx:Sender<Event>)->Sender<DetailRequest> {
    let (send,rx)=mpsc::channel::<DetailRequest>();
    thread::spawn(move||{while let Ok(mut r)=rx.recv() {
        while let Ok(n)=rx.try_recv() {r=n;}
        if tx.send(Event::Detail(detail(&cwd,&r))).is_err() {break}
    }}); send
}
pub fn open_engram(path:&Path)->rusqlite::Result<Connection> {Connection::open_with_flags(path,OpenFlags::SQLITE_OPEN_READ_ONLY|OpenFlags::SQLITE_OPEN_NO_MUTEX)}
/// `GET http://127.0.0.1:<port><path>` (HTTP/1.0, bounded by short timeouts) → JSON body of a 200 answer.
pub fn http_json(port:u16,path:&str)->Option<Value> {
    let mut s=std::net::TcpStream::connect_timeout(&([127,0,0,1],port).into(),Duration::from_millis(400)).ok()?;
    s.set_read_timeout(Some(Duration::from_millis(1200))).ok()?; s.set_write_timeout(Some(Duration::from_millis(400))).ok()?;
    s.write_all(format!("GET {path} HTTP/1.0\r\nHost: 127.0.0.1\r\nAccept: application/json\r\n\r\n").as_bytes()).ok()?;
    let mut buf=Vec::new(); s.take(256*1024).read_to_end(&mut buf).ok()?;
    let text=String::from_utf8_lossy(&buf); let (head,body)=text.split_once("\r\n\r\n")?;
    head.lines().next().is_some_and(|l|l.split_whitespace().nth(1)==Some("200")).then(||serde_json::from_str(body).ok()).flatten()
}
fn query_escape(s:&str)->String {s.bytes().map(|b|if b.is_ascii_alphanumeric()||b"-_.~".contains(&b) {(b as char).to_string()} else {format!("%{b:02X}")}).collect()}
pub fn engram(cwd:&Path)->Engram {
    let root=git_root(cwd).unwrap_or_else(||cwd.to_owned());
    let project=root.file_name().unwrap_or_default().to_string_lossy().into_owned();
    let path=std::env::var_os("ENGRAM_DB_PATH").map(PathBuf::from).unwrap_or_else(||dirs::home_dir().unwrap_or_default().join(".engram/engram.db"));
    let port=std::env::var("ENGRAM_PORT").ok().and_then(|p|p.parse().ok()).unwrap_or(7437);
    let mut e=Engram{online:http_json(port,"/health").is_some_and(|v|text(&v,"status")=="ok"),..Engram::default()};
    let db=open_engram(&path);
    let result=db.as_ref().map_err(|e|anyhow!("{e}")).and_then(|db|->Result<()>{
        e.count=Some(db.query_row("SELECT count(*) FROM observations WHERE project=?1 AND deleted_at IS NULL",[project.as_str()],|r|r.get(0))?);
        e.enrolled=db.query_row("SELECT count(*) FROM sync_enrolled_projects WHERE project=?1",[project.as_str()],|r|r.get::<_,i64>(0)).unwrap_or(0)>0;
        e.handoff=db.query_row("SELECT id,type,content,CAST(strftime('%s',created_at) AS INTEGER) FROM observations WHERE project=?1 AND type='session_summary' AND deleted_at IS NULL ORDER BY id DESC LIMIT 1",
            [project.as_str()],|r|Ok(Handoff{id:r.get(0)?,kind:r.get(1)?,lines:handoff_lines(&r.get::<_,String>(2)?),at:r.get(3)?})).ok();
        Ok(())
    });
    if let Err(err)=result {e.error=Some(err.to_string());}
    if e.online && e.enrolled {
        e.sync=http_json(port,&format!("/sync/status?project={}",query_escape(&project))).map(|v|{
            let last=v["last_sync_at"].as_str().and_then(|t|db.as_ref().ok()?.query_row("SELECT CAST(strftime('%s',?1) AS INTEGER)",[t],|r|r.get(0)).ok());
            (text(&v,"phase").to_string(),last)
        });
    }
    e.project=project; e
}
/// Resolve the configured store: explicit routing homes, the standard Pi home, or the
/// bundled layout of the linked agent home. Never inspects another user's store.
fn profile_models_path(agent:&Path)->Option<PathBuf> {
    let owner=std::env::var_os("GENTLE_PI_AGENT_HOME").filter(|v|!v.is_empty()).or_else(||std::env::var_os("PI_CODING_AGENT_DIR").filter(|v|!v.is_empty()));
    if owner.is_some_and(|p|Path::new(&p)!=agent) {return None}
    // The launcher hands these homes to the agent process only, so the sidecar reads them
    // from its own environment (gs3 forwards them); an override still wins over any default.
    for key in ["GENTLE_PI_CONFIG_HOME","GENTLE_SHELL_CONFIG_HOME"] {
        if let Some(p)=std::env::var_os(key).filter(|v|!v.is_empty()).map(PathBuf::from) {
            return p.is_absolute().then(||p.join("models.json"))
        }
    }
    let home=dirs::home_dir()?;
    if agent==home.join(".pi/agent") {return Some(home.join(".pi/gentle-ai/models.json"))}
    bundled_models(agent)
}
/// A bundled prefix owns both folders, `<prefix>/agent` and `<prefix>/config`: the store is
/// read only when that file exists, so a custom home never borrows another installation's.
fn bundled_models(agent:&Path)->Option<PathBuf> {
    if agent.file_name()?!="agent" {return None}
    let store=agent.parent()?.join("config/models.json");
    store.is_file().then_some(store)
}
/// Read the complete canonical routing object; never follow a file symlink.
fn profile_models(path:Option<PathBuf>)->(Option<Value>, &'static str) {
    let Some(path)=path else {return (None,"profiles: ruta desconocida")};
    let metadata=match fs::symlink_metadata(&path) {Ok(m)=>m,Err(_)=>return(None,"profiles: no disponible")};
    if !metadata.is_file() {return(None,"profiles: archivo inválido")}
    let value=fs::read(path).ok()
        .and_then(|b|serde_json::from_slice::<Value>(&b).ok()).filter(Value::is_object);
    if value.is_some() {(value,"/gentle:profiles · global")} else {(None,"profiles: JSON inválido")}
}
/// The profiles store (`profiles.json`) sits in the same config home as the routing store.
fn profiles_store_path(agent:&Path)->Option<PathBuf> { profile_models_path(agent).map(|p|p.with_file_name("profiles.json")) }
/// Read the `/gentle:profiles` store; never follow a symlink, never parse a non-object.
fn profiles_store(path:Option<PathBuf>)->Option<Value> {
    let path=path?;
    let metadata=fs::symlink_metadata(&path).ok()?;
    if !metadata.is_file() {return None}
    fs::read(&path).ok().and_then(|b|serde_json::from_slice::<Value>(&b).ok()).filter(Value::is_object)
}
/// PROF uses canonical configured routing; MD supplies names, tasks supply counts only.
/// The task history folder is shared by every session of the agent home, so only the tasks this
/// session and its subagents started belong on this pane: two sessions never repeat counts.
pub fn roster_of(session:&Path,cwd:&Path)->Option<Roster> {
    let dir=agent_dir(&session.to_string_lossy())?;
    let settings:Value=fs::read(dir.join("settings.json")).ok().and_then(|b|serde_json::from_slice(&b).ok()).unwrap_or(Value::Null);
    let files=|sub:&str,ext:&str|->Vec<PathBuf> {let mut v:Vec<PathBuf>=fs::read_dir(dir.join(sub)).into_iter().flatten().flatten().map(|e|e.path()).filter(|p|p.extension().is_some_and(|x|x==ext)).collect(); v.sort(); v};
    let agents:Vec<(String,String)>=files("agents","md").into_iter().take(64).filter_map(|p|Some((p.file_stem()?.to_string_lossy().into_owned(),fs::read_to_string(&p).ok()?))).collect();
    let parsed:Vec<Value>=files("gentle-agents/tasks","json").into_iter().filter_map(|p|serde_json::from_slice::<Value>(&fs::read(p).ok()?).ok()).map(|v|v["task"].clone()).filter(|t|t.is_object()).collect();
    let tasks:Vec<Value>=session_id(session).map(|sid|session_tasks(parsed,&sid)).unwrap_or_default();
    let (models,source)=profile_models(profile_models_path(&dir));
    let store=profiles_store(profiles_store_path(&dir));
    let pinned=git_root(cwd).and_then(|root|{
        let dir=git_dir(&root);
        let common=fs::read_to_string(dir.join("commondir")).ok().map(|s|dir.join(s.trim()));
        resolve_profile_pin(&common.unwrap_or(dir),&root,store.as_ref())
    });
    let now=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d|d.as_millis() as u64).unwrap_or(0);
    let mut r=configured_roster(roster(&settings,&agents,&tasks,now),models.as_ref(),source);
    r.profiles=profiles(store.as_ref()); r.pinned=pinned;
    Some(r)
}
/// The session id inside a session log's name: the `<uuid>` tail after the last `_`. The same
/// convention names the pane's own log and every `parentSessionId` in the task history, so this
/// is what ties a task to the session that started it.
fn session_id(path:&Path)->Option<String> {
    path.file_stem()?.to_string_lossy().rsplit_once('_').map(|(_,id)|id.to_string()).filter(|id|!id.is_empty())
}
/// The tasks of one session and of everything its subagents started: each task names the session
/// that launched it (`parentSessionId`) and, when it ran as its own session, the log of that
/// session (`sessionPath`). Following both links from the pane's session id covers the whole
/// descendant chain, not only the direct children.
fn session_tasks(mut parsed:Vec<Value>,sid:&str)->Vec<Value> {
    let mut frontier=vec![sid.to_string()]; let mut out=Vec::new();
    while !frontier.is_empty() {
        let mut next=Vec::new(); let mut i=0;
        while i<parsed.len() {
            if frontier.iter().any(|f|parsed[i]["parentSessionId"].as_str()==Some(f.as_str())) {
                let t=parsed.remove(i);
                if let Some(child)=t["sessionPath"].as_str().and_then(|p|session_id(Path::new(p))) {next.push(child)}
                out.push(t);
            } else {i+=1}
        }
        frontier=next;
    }
    out
}
/// A change inside the task history: Gentle AI rewrites one JSON per subagent task when it launches and
/// again when it ends, so watching that directory is what makes "en curso" mean something.
pub fn is_task_file(e:&notify::Event,agent_dir:&Path)->bool {
    let dir=agent_dir.join("gentle-agents").join("tasks");
    e.paths.iter().any(|p|p.starts_with(&dir))
}
/// A change to the profiles store: the package swaps it through a temp sibling and a rename,
/// so the temp name counts too. The store lives next to `models.json` in the config home.
pub fn is_profiles_file(e:&notify::Event,store_dir:&Path)->bool {
    e.paths.iter().any(|p|p.parent()==Some(store_dir) && p.file_name().is_some_and(|n|{let n=n.to_string_lossy(); n=="profiles.json"||n.starts_with(".profiles.json.")}))
}
/// A write to either pin layer (clone pin or repo declaration): both are swapped atomically
/// through a temp sibling, so the temp names count too.
pub fn is_profile_pin_file(e:&notify::Event)->bool {
    e.paths.iter().any(|p|{
        let name=p.file_name().unwrap_or_default().to_string_lossy();
        let parent=p.parent().and_then(Path::parent);
        p.parent().is_some_and(|d|d.ends_with("gentle-ai"))
            && parent.is_some_and(|g|g.file_name().is_some_and(|n|n==".git"||n==".pi"))
            && (name=="profile-pin.json"||name=="profile.json"||name.starts_with(".profile")&&name.ends_with(".tmp"))
    })
}
/// MCP servers configured for the session's Pi agent dir (`mcp.json`); names only.
pub fn mcp(session:&Path)->Option<Vec<Mcp>> {
    let dir=agent_dir(&session.to_string_lossy())?;
    let v:Value=serde_json::from_slice(&fs::read(dir.join("mcp.json")).ok()?).ok()?;
    Some(mcp_servers(&v))
}
pub fn read_tail(path:&Path,tail:&mut Tail)->Result<()> {
    let mut f=fs::File::open(path)?; let reset=f.metadata()?.len()<tail.offset;
    if reset {*tail=Tail::default();}
    f.seek(SeekFrom::Start(tail.offset))?; let mut bytes=Vec::new(); f.read_to_end(&mut bytes)?; tail.ingest(&bytes,false);
    // Context window only depends on the model: re-read the model stores only when it changes.
    if tail.limit_model.as_deref()==Some(tail.session.model.as_str()) {return Ok(())}
    tail.limit_model=Some(tail.session.model.clone());
    if let Some((agent,_))=path.to_string_lossy().split_once("/sessions/") {
        // The cpamc cache is what Pi registers at runtime (its footer ctx % uses it); the store is a fallback.
        tail.session.limit=["cpamc-models-cache.json","models-store.json"].iter().find_map(|name|{
            let v:Value=serde_json::from_slice(&fs::read(Path::new(agent).join(name)).ok()?).ok()?;
            context_window(&v,&tail.session.model)
        });
    }
    Ok(())
}
pub fn request(socket:&Path,method:&str,params:Value)->Result<Value> {
    let mut stream=UnixStream::connect(socket)?; stream.set_read_timeout(Some(Duration::from_secs(3)))?; stream.set_write_timeout(Some(Duration::from_secs(3)))?;
    writeln!(stream,"{}",json!({"id":"hud","method":method,"params":params}))?;
    let mut line=String::new(); BufReader::new(stream).read_line(&mut line)?;
    let v:Value=serde_json::from_str(&line)?;
    if !v["error"].is_null() {return Err(anyhow!("{}",v["error"]))} Ok(v["result"].clone())
}
/// The only pane-control exception: fixed F9 to the pane bound at startup, after focus succeeds.
fn model_picker_requests(id:&str)->Vec<(&'static str,Value)> {
    vec![
        ("pane.focus",json!({"pane_id":id})),
        ("pane.send_keys",json!({"pane_id":id,"keys":["f9"]})),
    ]
}
/// `/gentle:profiles` to the bound pane: the command travels as literal text (send_keys rejects text
/// with invalid_key) and only the Enter press travels as a key, after focus succeeds.
fn profiles_command_requests(id:&str)->Vec<(&'static str,Value)> {
    vec![
        ("pane.focus",json!({"pane_id":id})),
        ("pane.send_text",json!({"pane_id":id,"text":"/gentle:profiles"})),
        ("pane.send_keys",json!({"pane_id":id,"keys":["Enter"]})),
    ]
}
/// The session profile channel: the profile-server extension of the agent's Pi
/// process listens on /tmp/gs3-profile-<uid>-<pane>.sock (pane = the agent pane the
/// rail watches). The base is a fixed /tmp on purpose: the bundled shell rebuilds
/// the environment, so TMPDIR differs between this rail and the agent process.
/// One JSON request per connection; the profile name travels as data, never as
/// command text — nothing is typed into the agent pane.
/// The process owner's uid, read once: the channel socket is keyed and verified by it.
fn unix_uid()->Option<u32> {use std::sync::OnceLock;static U:OnceLock<Option<u32>>=OnceLock::new();*U.get_or_init(||std::process::Command::new("id").arg("-u").output().ok().and_then(|o|String::from_utf8(o.stdout).ok()).and_then(|s|s.trim().parse().ok()))}

pub fn profile_channel_socket(pane:&str,uid:u32)->PathBuf {
    // Defaults to /tmp (the bundled shell rebuilds TMPDIR); GS3_PROFILE_SOCKET_DIR lets an
    // isolated runner keep both sides on a short scratch directory.
    let base=std::env::var("GS3_PROFILE_SOCKET_DIR").ok().filter(|p| !p.trim().is_empty()).map(PathBuf::from).unwrap_or_else(||PathBuf::from("/tmp"));
    base.join(format!("gs3-profile-{uid}-{pane}.sock"))
}
/// One channel round trip: `get` (the session's current binding, or nothing) or
/// `use {name}` (bind the profile to the session; the reply carries the profile's
/// model map). A missing socket means the extension is not loaded: the caller
/// renders that, it is not an error to report.
fn profile_channel(socket:&Path,payload:Value)->Result<Value> {
    // The socket path is predictable from the pane id, so the owner is checked before any
    // byte is written: another local user could otherwise answer for the extension and hand
    // the rail a forged binding.
    let meta=fs::symlink_metadata(socket)?;
    use std::os::unix::fs::{FileTypeExt, MetadataExt};
    if !meta.file_type().is_socket() {return Err(anyhow!("not a socket"))}
    let owner=meta.uid();
    if owner!=unix_uid().ok_or_else(||anyhow!("no uid"))? {return Err(anyhow!("socket owned by another user"))}
    if meta.mode()&0o077!=0 {return Err(anyhow!("socket is group/world accessible"))}
    let mut stream=UnixStream::connect(socket)?;
    stream.set_read_timeout(Some(Duration::from_secs(3)))?; stream.set_write_timeout(Some(Duration::from_secs(3)))?;
    writeln!(stream,"{payload}")?;
    let mut line=String::new(); BufReader::new(&stream).read_line(&mut line)?;
    let v:Value=serde_json::from_str(&line)?;
    if !v["error"].is_null() {return Err(anyhow!("{}",v["error"]))} Ok(v["result"].clone())
}
/// The session's current profile binding: `Some(name)` (plus the profile's model
/// map when the store still knows it), `None` (no binding yet), or an error
/// (channel down — extension not loaded, or the socket was lost).
pub fn profile_channel_get(socket:&Path)->Result<Option<Value>> {
    let result=profile_channel(socket,json!({"id":"hud","method":"get","params":{}}))?;
    Ok(if result["profile"].is_null() {None} else {Some(result["profile"].clone())})
}
/// Ask the agent's session to load a profile. The name already passed the
/// package's own sendable rule at render time; here it is data on a 0600 socket,
/// never a command. The reply is the loaded profile (name + model map).
pub fn profile_channel_use(socket:&Path,name:&str)->Result<Value> {
    profile_channel(socket,json!({"id":"hud","method":"use","params":{"name":name}}))
}
/// Explicit clicks only; coalesce pending requests and discard clicks within one second of the last attempt.
fn pane_worker(socket:PathBuf,requests:Vec<(&'static str,Value)>)->Sender<Instant> {
    let (send,rx)=mpsc::channel::<Instant>();
    thread::spawn(move||{
        let mut last:Option<Instant>=None;
        while let Ok(mut at)=rx.recv() {
            while let Ok(next)=rx.try_recv() {at=next;}
            if last.is_some_and(|prev|at.saturating_duration_since(prev)<Duration::from_secs(1)) {continue}
            last=Some(at);
            for (method,params) in &requests {
                if request(&socket,method,params.clone()).is_err() {break}
            }
        }
    }); send
}
/// Profile clicks share one process-wide 1 s window: a double click spawns two
/// click intents, and two channel requests racing each other would double-load
/// the profile; the second click inside the window is simply discarded.
static USE_PROFILE_WINDOW: Mutex<Option<Instant>> = Mutex::new(None);
/// A profile click: one `use` request to the session profile channel per click
/// intent, throttled by the shared window. The reply (the loaded profile's name
/// and model map) is emitted as an event; the render thread never blocks on it.
pub fn profile_channel_worker(socket:PathBuf,tx:Sender<crate::core::Event>) {
    let (send,rx)=mpsc::channel::<String>();
    PROFILE_CLICK_SENDER.get_or_init(||send);
    thread::spawn(move||{
        while let Ok(name)=rx.recv() {
            let mut latest=name;
            while let Ok(next)=rx.try_recv() {latest=next;}
            let name=latest;
            {
                let mut cell=USE_PROFILE_WINDOW.lock().unwrap();
                if cell.is_some_and(|prev|Instant::now().saturating_duration_since(prev)<Duration::from_secs(1)) {continue}
                *cell=Some(Instant::now());
            }
            // The reply is {ok, profile} on success and {ok:false, error} on a rejected name;
            // the event must carry the profile object itself (name + model map), never the
            // wrapper: the cards read `name` and `models` at the top level.
            match profile_channel_use(&socket,&name) {
                Ok(reply) if reply["ok"]==serde_json::json!(true)=>{
                    // `live` says whether the session really switched model: the card shows the
                    // profile's configured routing either way, so the outcome has to travel.
                    let mut profile=reply["profile"].clone();
                    profile["live"]=reply["live"].clone();
                    let _=tx.send(crate::core::Event::ProfileChannel(profile));
                }
                // A rejected name leaves the previous binding in place: re-read it instead of
                // pretending the session has no profile.
                Ok(_)=>{let _=tx.send(match profile_channel_get(&socket){Ok(p)=>crate::core::Event::ProfileChannel(p.unwrap_or(serde_json::Value::Bool(false))),Err(_)=>crate::core::Event::ProfileChannelDown});}
                Err(_)=>{let _=tx.send(crate::core::Event::ProfileChannelDown);}
            }
        }
    });
}
/// The render thread's handle for click intents: a send enqueues the profile
/// name; the worker throttles it and answers through the event channel.
pub fn profile_click(name:String) {
    if let Some(send)=PROFILE_CLICK_SENDER.get() {let _=send.send(name);}
}
static PROFILE_CLICK_SENDER: std::sync::OnceLock<Sender<String>> = std::sync::OnceLock::new();
pub fn model_picker(socket:PathBuf,id:String)->Sender<Instant> { pane_worker(socket,model_picker_requests(&id)) }
pub fn profiles_command(socket:PathBuf,id:String)->Sender<Instant> { pane_worker(socket,profiles_command_requests(&id)) }
pub fn pane(socket:&Path,id:&str)->Result<Option<Value>> {
    let found=match request(socket,"pane.get",json!({"pane_id":id})) {
        Ok(v) if v["pane"].is_object()=>Some(v["pane"].clone()),
        _=>{let v=request(socket,"pane.list",json!({}))?;
            let list=v["panes"].as_array().or_else(||v.as_array()).context("invalid pane.list")?;
            list.iter().find(|v|text(v,"pane_id")==id).cloned()}
    };
    Ok(found.map(|mut p|{
        // Panes only carry workspace_id; the human label lives on the workspace.
        if let Ok(w)=request(socket,"workspace.get",json!({"workspace_id":text(&p,"workspace_id")})) {p["workspace_label"]=w["workspace"]["label"].clone();}
        p
    }))
}
pub fn herdr(socket:PathBuf,id:String,tx:Sender<Event>) {
    thread::spawn(move||{
        let mut backoff=1;
        loop {
            let result=(||->Result<()>{
                let mut stream=UnixStream::connect(&socket)?;
                stream.set_write_timeout(Some(Duration::from_secs(3)))?;
                writeln!(stream,"{}",json!({"id":"events","method":"events.subscribe","params":{"subscriptions":[{"type":"pane.updated"},{"type":"pane.closed"},{"type":"pane.agent_status_changed","pane_id":id},{"type":"workspace.focused"}]}}))?;
                for line in BufReader::new(stream).lines() {
                    let v:Value=serde_json::from_str(&line?)?; let (kind,event)=parse_wire(&v);
                    if kind=="error" {return Err(anyhow!("subscription rejected"))}
                    if kind=="started" {
                        tx.send(Event::Connection(true))?; backoff=1;
                        match pane(&socket,&id)? {Some(p)=>tx.send(Event::Pane(p))?,None=>{tx.send(Event::Closed(id.clone()))?;return Ok(())}}
                    }
                    let relevant=match &event {Some(Event::Pane(p))=>text(p,"pane_id")==id,Some(Event::Status(p,_))|Some(Event::Closed(p))=>p==&id,_=>false};
                    let closed=matches!(&event,Some(Event::Closed(p)) if p==&id);
                    if let Some(e)=event {tx.send(e)?;}
                    if closed {return Ok(())}
                    if relevant {if let Ok(p)=pane(&socket,&id) {match p {Some(p)=>tx.send(Event::Pane(p))?,None=>{tx.send(Event::Closed(id.clone()))?;return Ok(())}}}}
                }
                Err(anyhow!("socket closed"))
            })();
            if result.is_ok() {break}
            if tx.send(Event::Connection(false)).is_err() {break}
            thread::sleep(Duration::from_secs(backoff)); backoff=(backoff*2).min(30);
        }
    });
}

pub enum Local { Refresh, Session(Option<String>), Changed(notify::Event), More(usize) }
/// Pi creates a new session file on `/new` / `/resume`, but herdr does not always emit
/// `pane_updated` for the change: a new `.jsonl` next to the bound session re-queries the pane.
pub fn is_new_session_file(e:&notify::Event,session:&Path)->bool {
    matches!(e.kind,notify::EventKind::Create(_)) && e.paths.iter().any(|p|p!=session && p.parent()==session.parent() && p.extension().is_some_and(|x|x=="jsonl"))
}
pub fn local(cwd:PathBuf,socket:PathBuf,pane_id:String,tx:Sender<Event>)->Sender<Local> {
    let (send,rx)=mpsc::channel(); let notify_tx=send.clone();
    thread::spawn(move||{
        let mut watcher=notify::recommended_watcher(move|r:notify::Result<notify::Event>|{if let Ok(e)=r {let _=notify_tx.send(Local::Changed(e));}}).ok();
        if let (Some(w),Some(root))=(&mut watcher,git_root(&cwd)) {
            let dir=git_dir(&root);
            for p in [dir.join("HEAD"),dir.join("index")] {let _=w.watch(&p,RecursiveMode::NonRecursive);}
            let _=w.watch(&dir.join("refs"),RecursiveMode::Recursive);
            // Watch only direct gitdir entries too: HEAD/index are commonly atomically replaced.
            let _=w.watch(&dir,RecursiveMode::NonRecursive);
            if let Ok(common)=fs::read_to_string(dir.join("commondir")) {let _=w.watch(&dir.join(common.trim()).join("refs"),RecursiveMode::Recursive);}
        }
        let mut session:Option<PathBuf>=None; let mut tail=Tail::default(); let mut git_deadline:Option<Instant>=None;
        // The task history has its own debounce: five tasks writing at once must not mean five reads.
        let mut roster_deadline:Option<Instant>=None; let mut tasks_dir:Option<PathBuf>=None;
        // The profiles store rides the same roster refresh; its dir is watched for the atomic swap.
        let mut profiles_dir:Option<PathBuf>=None;
        // Pin layers live under the gitdir and the worktree root; both are watched for the swap.
        let mut pin_dirs:Vec<PathBuf>=Vec::new();
        // Last MCP list sent; matches State::new so unchanged lists are never re-sent.
        let mut mcp_sent:Option<Vec<Mcp>>=None;
        let send_mcp=|session:&Option<PathBuf>,sent:&mut Option<Vec<Mcp>>|{let now=session.as_deref().and_then(mcp); if now!=*sent {*sent=now.clone(); let _=tx.send(Event::Mcp(now));}};
        // Same rule for the PROF roster (agent dir files): only a changed roster becomes an event.
        let mut roster_sent:Option<Roster>=None;
        let mut page=LOG_PAGE;
        let send_roster=|session:&Option<PathBuf>,sent:&mut Option<Roster>|{let now=session.as_deref().and_then(|s|roster_of(s,&cwd)); if now!=*sent {*sent=now.clone(); let _=tx.send(Event::Roster(now));}};
        loop {
            let deadline=match (git_deadline,roster_deadline) {(None,None)=>None,(Some(a),None)=>Some(a),(None,Some(b))=>Some(b),(Some(a),Some(b))=>Some(a.min(b))};
            let msg=match deadline {Some(t)=>rx.recv_timeout(t.saturating_duration_since(Instant::now())),None=>rx.recv().map_err(|_|mpsc::RecvTimeoutError::Disconnected)};
            match msg {
                // The graph keeps the page it was asked for: a refresh must not shrink the list under the wheel.
                Ok(Local::More(n))=>{page=n.clamp(LOG_PAGE,LOG_MAX); let _=tx.send(Event::Git(git_log(&cwd,page)));}
                Ok(Local::Refresh)=>{
                    // The tasks directory only exists after the first task: watching it again costs nothing.
                    if let (Some(w),Some(d))=(&mut watcher,&tasks_dir) {let _=w.watch(d,RecursiveMode::NonRecursive);}
                    if let (Some(w),Some(d))=(&mut watcher,&profiles_dir) {let _=w.watch(d,RecursiveMode::NonRecursive);}
                    let _=tx.send(Event::Git(git_log(&cwd,page))); let _=tx.send(Event::Engram(engram(&cwd))); send_mcp(&session,&mut mcp_sent); send_roster(&session,&mut roster_sent); let _=tx.send(Event::Stack(stack(&cwd,session.as_deref())));}
                Ok(Local::Session(path))=>{
                    if let (Some(w),Some(p))=(&mut watcher,&session) {let _=w.unwatch(p.parent().unwrap_or(p));}
                    if let (Some(w),Some(d))=(&mut watcher,&tasks_dir) {let _=w.unwatch(d);}
                    if let (Some(w),Some(d))=(&mut watcher,&profiles_dir) {let _=w.unwatch(d);}
                    for d in pin_dirs.drain(..) {if let Some(w)=&mut watcher {let _=w.unwatch(&d);}}
                    session=path.map(PathBuf::from); tail=Tail::default();
                    if let Some(p)=&session {
                        if let Some(w)=&mut watcher {let _=w.watch(p.parent().unwrap_or(p),RecursiveMode::NonRecursive);}
                        let _=read_tail(p,&mut tail);
                    }
                    tasks_dir=session.as_deref().and_then(|p|agent_dir(&p.to_string_lossy())).map(|d|d.join("gentle-agents").join("tasks"));
                    profiles_dir=session.as_deref().and_then(|p|agent_dir(&p.to_string_lossy())).and_then(|d|profiles_store_path(&d)).and_then(|p|p.parent().map(Path::to_path_buf));
                    // Pin layers: clone pin under the git common dir, repo declaration at the worktree root.
                    pin_dirs=git_root(&cwd).map(|root|{
                        let dir=git_dir(&root);
                        let common=fs::read_to_string(dir.join("commondir")).ok().map(|s|dir.join(s.trim()));
                        vec![common.unwrap_or(dir).join("gentle-ai"),root.join(".pi").join("gentle-ai")]
                    }).unwrap_or_default();
                    for d in pin_dirs.iter() {if let Some(w)=&mut watcher {let _=w.watch(d,RecursiveMode::NonRecursive);}}
                    if let (Some(w),Some(d))=(&mut watcher,&tasks_dir) {let _=w.watch(d,RecursiveMode::NonRecursive);}
                    if let (Some(w),Some(d))=(&mut watcher,&profiles_dir) {let _=w.watch(d,RecursiveMode::NonRecursive);}
                    let _=tx.send(Event::Session(session.as_ref().map(|p|p.to_string_lossy().into_owned()),tail.session.clone()));
                    send_mcp(&session,&mut mcp_sent); send_roster(&session,&mut roster_sent);
                    let _=tx.send(Event::Stack(stack(&cwd,session.as_deref())));
                }
                Ok(Local::Changed(e))=>{
                    if !matches!(e.kind,notify::EventKind::Create(_)|notify::EventKind::Modify(_)|notify::EventKind::Remove(_)) {continue}
                    if session.as_ref().and_then(|p|agent_dir(&p.to_string_lossy())).is_some_and(|d|is_task_file(&e,&d)) {roster_deadline=Some(Instant::now()+Duration::from_millis(300));}
                    else if is_profile_pin_file(&e) {roster_deadline=Some(Instant::now()+Duration::from_millis(300));}
                    else if profiles_dir.as_ref().is_some_and(|d|is_profiles_file(&e,d)) {roster_deadline=Some(Instant::now()+Duration::from_millis(300));}
                    else if session.as_ref().is_some_and(|p|is_new_session_file(&e,p)) {if let Ok(Some(p))=pane(&socket,&pane_id) {let _=tx.send(Event::Pane(p));}}
                    else if session.as_ref().is_some_and(|p|e.paths.contains(p)) {if let Some(p)=&session {let _=read_tail(p,&mut tail); let _=tx.send(Event::Session(Some(p.to_string_lossy().into_owned()),tail.session.clone()));}}
                    else if e.paths.iter().any(|p|{let name=p.file_name().unwrap_or_default(); name=="HEAD"||name=="index"||p.components().any(|c|c.as_os_str()=="refs")}) {git_deadline=Some(Instant::now()+Duration::from_millis(300));}
                }
                Err(mpsc::RecvTimeoutError::Timeout)=>{
                    let now=Instant::now();
                    if git_deadline.is_some_and(|t|t<=now) {git_deadline=None; let _=tx.send(Event::Git(git_snapshot(&cwd)));}
                    if roster_deadline.is_some_and(|t|t<=now) {roster_deadline=None; send_roster(&session,&mut roster_sent);}
                }
                Err(_)=>break
            }
        }
    }); send
}

fn github(cwd:&Path)->bool {git(cwd,&["remote","get-url","origin"]).is_ok_and(|s|s.contains("github.com"))}
fn pr_current(cwd:&Path)->Result<Vec<String>> {
    let output=run(cwd,"gh",&["pr","view","--json","number,title,state,reviewDecision,statusCheckRollup"],8);
    match output {
        Err(e) if e.to_string().contains("no pull requests found")=>Ok(vec![format!("no PR for {}",git(cwd,&["branch","--show-current"]).unwrap_or_default())]),
        Err(e)=>Err(e),
        Ok(s)=>{let v:Value=serde_json::from_str(&s)?; let mut counts=[0;3];
            if let Some(checks)=v["statusCheckRollup"].as_array() {for c in checks {let state=c["conclusion"].as_str().filter(|s|!s.is_empty()).unwrap_or(text(c,"state")); let i=match state {"SUCCESS"|"NEUTRAL"|"SKIPPED"=>0,"FAILURE"|"ERROR"|"CANCELLED"|"TIMED_OUT"|"ACTION_REQUIRED"=>1,_=>2}; counts[i]+=1;}}
            Ok(vec![format!("#{} {}",v["number"],text(&v,"title")),format!("{} · {}",text(&v,"state"),text(&v,"reviewDecision")),format!("checks ✓{} ✗{} …{}",counts[0],counts[1],counts[2])])}
    }
}
// One PR cycle = current-branch PR + open count; cycles are spaced by PR_MIN_GAP seconds.
fn pr_cycle(cwd:&Path)->Vec<String> {
    let mut lines=pr_current(cwd).unwrap_or_else(|e|vec![format!("PR error: {e}")]);
    lines.push(open_count(cwd)); lines
}
pub fn pr_once(cwd:&Path)->Vec<String> {
    if !github(cwd) {return vec!["no GitHub origin".into()]}
    pr_cycle(cwd)
}
fn open_count(cwd:&Path)->String {
    match run(cwd,"gh",&["pr","list","--state","open","--json","number","--limit","100"],8).and_then(|s|Ok(serde_json::from_str::<Vec<Value>>(&s)?.len())) {
        Ok(n)=>format!("open PRs: {n}{}",if n==100{"+"}else{""}),Err(e)=>format!("open PRs error: {e}")
    }
}
pub fn prs(cwd:PathBuf,tx:Sender<Event>)->Sender<()> {
    let (send,rx)=mpsc::channel();
    thread::spawn(move||{
        if !github(&cwd) {let _=tx.send(Event::Pr(vec!["no GitHub origin".into()])); return}
        let start=Instant::now(); let mut last:Option<u64>=None; let mut due=0; let mut attempt=0;
        loop {
            let now=start.elapsed().as_secs();
            if now>=due && pr_due(last,now) {
                if tx.send(Event::Pr(pr_cycle(&cwd))).is_err() {break}
                let done=start.elapsed().as_secs(); last=Some(done); due=done+pr_delay(attempt); attempt+=1;
            }
            let wait=due.saturating_sub(start.elapsed().as_secs()).max(1);
            match rx.recv_timeout(Duration::from_secs(wait)) {
                // Triggers (start, HEAD change, turn end, `r`) coalesce and reset the idle backoff.
                Ok(())=>{attempt=0; due=pr_trigger_due(last,start.elapsed().as_secs());}
                Err(mpsc::RecvTimeoutError::Timeout)=>{},Err(_)=>break
            }
        }
    }); send
}


/// A fixture directory short enough to bind a Unix socket inside it: `sun_path` is 104 bytes on
/// macOS and 108 on Linux, so `temp_dir()` (long under CI and isolated scratch copies) overflows
/// it. `GS3_TEST_TMP` overrides the base when a runner needs its own root, and is refused when it
/// is too long to be useful.
#[cfg(test)]
fn fixture(tag:&str)->PathBuf {
    let base=match std::env::var("GS3_TEST_TMP") {Ok(p) if !p.is_empty() && p.len()<40=>PathBuf::from(p),_=>PathBuf::from("/tmp")};
    let dir=base.join(format!("gs3-{tag}-{}",std::process::id()));
    let _=fs::remove_dir_all(&dir);
    let _=fs::create_dir_all(&dir);
    dir
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn model_picker_requests_are_bound_focus_then_f9() {
        for pane in ["w1:p1", "bound-other"] {
            assert_eq!(model_picker_requests(pane), vec![
                ("pane.focus", json!({"pane_id":pane})),
                ("pane.send_keys", json!({"pane_id":pane,"keys":["f9"]})),
            ]);
        }
    }
    #[test]
    fn publish_clicks_carry_only_sendable_names() {
        // The click intent never carries an invalid name: Pi's own rule decides what can travel.
        assert!(is_valid_profile_name("uno"));
        assert!(is_valid_profile_name("Grok-muse"));
        assert!(is_valid_profile_name("a.b-c_d"));
        for bad in ["", " dos", "dos ", "un o", "../x", "a\tb", "__proto__", "constructor", "prototype", "-uno", ".uno", "unoñe", &"x".repeat(65)] {
            assert!(!is_valid_profile_name(bad), "{bad:?} no puede viajar");
        }
    }
    #[test]
    fn profiles_command_requests_are_focus_text_then_enter() {
        // send_keys rejects literal text (invalid_key), so the command travels as text and only the
        // Enter press travels as a key; the rail sends the fixed literal, never a profile name.
        for pane in ["w1:p1", "bound-other"] {
            assert_eq!(profiles_command_requests(pane), vec![
                ("pane.focus", json!({"pane_id":pane})),
                ("pane.send_text", json!({"pane_id":pane,"text":"/gentle:profiles"})),
                ("pane.send_keys", json!({"pane_id":pane,"keys":["Enter"]})),
            ]);
        }
    }
    #[test]
    fn profile_channel_socket_path_is_deterministic() {
        // Both sides derive the same path independently: the extension from the
        // gs3-herdr-pane flag, the rail from its --pane. Fixed /tmp base, uid in the name.
        assert_eq!(profile_channel_socket("w2:p4D",501), PathBuf::from("/tmp/gs3-profile-501-w2:p4D.sock"));
        assert_eq!(profile_channel_socket("w1:p1",0), PathBuf::from("/tmp/gs3-profile-0-w1:p1.sock"));
    }
    #[test]
    fn profile_channel_get_and_use_round_trip_through_a_fake_socket() {
        // A fake profile-server: `get` returns a bound profile, `use` records the
        // name it received (as JSON data on the socket, never as command text).
        let dir=fixture("profchan");
         fs::create_dir_all(&dir).unwrap();
        let socket=dir.join("channel.sock");
        let listener=std::os::unix::net::UnixListener::bind(&socket).unwrap();
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&socket,std::fs::Permissions::from_mode(0o600)).unwrap();
        let used=std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        let seen=used.clone();
        let serving=listener.try_clone().unwrap();
        thread::spawn(move||{
            let listener=serving;
            while let Ok((mut stream,_))=listener.accept() {
                let mut line=String::new();
                {let mut r=BufReader::new(stream.try_clone().unwrap()); let _=r.read_line(&mut line);}
                let v:Value=serde_json::from_str(&line).unwrap();
                let reply=match text(&v,"method") {
                    "get"=>json!({"id":v["id"],"result":{"profile":{"name":"alpha","models":{"orchestrator":{"model":"p/host"}}}}}),
                    "use"=>{seen.lock().unwrap().push(v["params"]["name"].as_str().unwrap_or_default().to_string()); json!({"id":v["id"],"result":{"ok":true,"profile":{"name":v["params"]["name"],"models":{}}}})},
                    _=>json!({"id":v["id"],"error":{"message":"unknown method"}}),
                };
                let _=writeln!(stream,"{reply}");
            }
        });
        // get: the session's current binding comes back as the profile object.
        let bound=profile_channel_get(&socket).unwrap().unwrap();
        assert_eq!(text(&bound,"name"),"alpha");
        // use: the name travels as JSON data and the reply confirms the load.
        let loaded=profile_channel_use(&socket,"beta").unwrap();
        assert_eq!(loaded["ok"],json!(true));
        assert_eq!(*used.lock().unwrap(), vec!["beta".to_string()]);
        // A down channel is an error the caller renders, never a panic.
        let dead=dir.join("dead.sock");
        assert!(profile_channel_get(&dead).is_err());
        drop(listener); fs::remove_dir_all(&dir).unwrap();
    }
    #[test]
    fn profile_channel_refuses_a_non_socket_or_open_socket() {
        // The path is predictable from the pane id: a plain file, or a socket another user
        // could talk to, must be refused before the rail writes anything into it.
        let dir=fixture("profbad");
         fs::create_dir_all(&dir).unwrap();
        let impostor=dir.join("plain.sock");
        fs::write(&impostor,"{}").unwrap();
        let err=profile_channel_get(&impostor).err().map(|e|e.to_string()).unwrap_or_default();
        assert!(err.contains("not a socket"),"un archivo común no es canal: {err}");
        let open=dir.join("open.sock");
        let listener=std::os::unix::net::UnixListener::bind(&open).unwrap();
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&open,std::fs::Permissions::from_mode(0o666)).unwrap();
        let err=profile_channel_get(&open).err().map(|e|e.to_string()).unwrap_or_default();
        assert!(err.contains("group/world"),"un socket abierto al grupo/mundo se rechaza: {err}");
        drop(listener); fs::remove_dir_all(&dir).unwrap();
    }
    #[test]
    fn profile_clicks_share_the_one_second_window() {
        // Two click intents must double-load nothing: the shared window discards
        // the second click whatever worker moment it lands on.
        let dir=fixture("profclk");
         fs::create_dir_all(&dir).unwrap();
        let socket=dir.join("channel.sock");
        let listener=std::os::unix::net::UnixListener::bind(&socket).unwrap();
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&socket,std::fs::Permissions::from_mode(0o600)).unwrap();
        let count=std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let seen=count.clone();
        thread::spawn(move||{
            while let Ok((mut stream,_))=listener.accept() {
                let mut line=String::new();
                {let mut r=BufReader::new(stream.try_clone().unwrap()); let _=r.read_line(&mut line);}
                if serde_json::from_str::<Value>(&line).map(|v|text(&v,"method")=="use").unwrap_or(false) {seen.fetch_add(1,std::sync::atomic::Ordering::SeqCst);}
                let _=writeln!(stream,"{}",json!({"result":{"ok":true,"profile":{"name":"uno","models":{}}}}));
            }
        });
        let (tx,rx)=mpsc::channel::<crate::core::Event>();
        profile_channel_worker(socket.clone(),tx);
        profile_click("uno".into());
        thread::sleep(Duration::from_millis(150));
        profile_click("uno".into());
        thread::sleep(Duration::from_millis(350));
        assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst),1,"un doble clic carga una sola vez");
        // After the shared window a click loads again.
        thread::sleep(Duration::from_millis(600));
        profile_click("uno".into());
        thread::sleep(Duration::from_millis(150));
        assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst),2);
        // The reply arrives as an event carrying the loaded profile.
        let mut events=0;
        // The event carries the profile itself: the cards read `name` at the top level.
        while let Ok(crate::core::Event::ProfileChannel(v))=rx.try_recv() {
            if v.is_boolean() {continue}
            assert_eq!(text(&v,"name"),"uno","el evento lleva el perfil desempaquetado"); events+=1;
        }
        assert_eq!(events,2,"cada carga responde con un evento");
        fs::remove_dir_all(&dir).unwrap();
    }
    #[test]
    fn profiles_command_clicks_are_coalesced_and_throttled() {
        let dir=fixture("profcmd");
         fs::create_dir_all(&dir).unwrap();
        let socket=dir.join("herdr.sock");
        let listener=std::os::unix::net::UnixListener::bind(&socket).unwrap();
        let focus=std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let seen=focus.clone();
        thread::spawn(move||{
            while let Ok((mut stream,_))=listener.accept() {
                let mut line=String::new();
                {let mut r=BufReader::new(stream.try_clone().unwrap()); let _=r.read_line(&mut line);}
                if serde_json::from_str::<Value>(&line).map(|v|text(&v,"method")=="pane.focus").unwrap_or(false) {seen.fetch_add(1,std::sync::atomic::Ordering::SeqCst);}
                let _=writeln!(stream,"{}",json!({"result":{"type":"ok"}}));
            }
        });
        let send=profiles_command(socket.clone(),"w1:p1".into());
        // A double click is one command: the second click inside the 1 s window is dropped.
        send.send(Instant::now()).unwrap();
        thread::sleep(Duration::from_millis(200));
        send.send(Instant::now()).unwrap();
        thread::sleep(Duration::from_millis(600));
        assert_eq!(focus.load(std::sync::atomic::Ordering::SeqCst),1,"el doble clic no envía dos veces");
        // After the window a click sends again.
        thread::sleep(Duration::from_millis(600));
        send.send(Instant::now()).unwrap();
        thread::sleep(Duration::from_millis(200));
        assert_eq!(focus.load(std::sync::atomic::Ordering::SeqCst),2);
        drop(send); fs::remove_dir_all(&dir).unwrap();
    }
    #[test]
    fn session_files_switch_append_and_truncate() {
        let dir=fixture("tail");
        let first=dir.join("first.jsonl"); let second=dir.join("second.jsonl");
        let line=b"{\"type\":\"message\",\"message\":{\"role\":\"assistant\",\"usage\":{\"input\":7}}}\n";
        fs::write(&first,line).unwrap(); fs::write(&second,line).unwrap();
        let mut tail=Tail::default(); read_tail(&first,&mut tail).unwrap();
        assert_eq!(tail.session.input,7);
        let mut f=fs::OpenOptions::new().append(true).open(&first).unwrap();
        f.write_all(&line[..20]).unwrap(); read_tail(&first,&mut tail).unwrap(); assert_eq!(tail.session.input,7);
        f.write_all(&line[20..]).unwrap(); read_tail(&first,&mut tail).unwrap(); assert_eq!(tail.session.input,14);
        fs::write(&first,line).unwrap(); read_tail(&first,&mut tail).unwrap(); assert_eq!(tail.session.input,7);
        let mut state=State::new("p".into(),dir.clone()); state.session=tail.session.clone();
        apply(&mut state,Event::Pane(json!({"pane_id":"p","agent_session":{"value":second}})));
        assert_eq!(state.session.input,0); assert!(state.read_session);
        tail=Tail::default(); read_tail(Path::new(state.session_path.as_ref().unwrap()),&mut tail).unwrap();
        let path=state.session_path.clone(); apply(&mut state,Event::Session(path,tail.session)); assert_eq!(state.session.input,7);
        drop(f); fs::remove_file(first).unwrap(); fs::remove_file(second).unwrap(); fs::remove_dir(dir).unwrap();
    }
    #[test]
    fn new_sibling_session_file_triggers_requery() {
        let ev=|kind,p:&str|notify::Event{kind,paths:vec![PathBuf::from(p)],attrs:Default::default()};
        let create=notify::EventKind::Create(notify::event::CreateKind::File);
        let modify=notify::EventKind::Modify(notify::event::ModifyKind::Any);
        let cur=Path::new("/s/dir/a.jsonl");
        assert!(is_new_session_file(&ev(create,"/s/dir/b.jsonl"),cur));
        assert!(!is_new_session_file(&ev(create,"/s/dir/a.jsonl"),cur));
        assert!(!is_new_session_file(&ev(modify,"/s/dir/b.jsonl"),cur));
        assert!(!is_new_session_file(&ev(create,"/s/dir/b.tmp"),cur));
        assert!(!is_new_session_file(&ev(create,"/s/other/b.jsonl"),cur));
    }
    #[test]
    fn subprocesses_disable_git_optional_locks() {
        let out=run(&std::env::temp_dir(),"/bin/sh",&["-c","printf '%s' \"$GIT_OPTIONAL_LOCKS\""],3).unwrap();
        assert_eq!(out,"0");
    }
    #[test]
    fn pane_lookup_adds_workspace_label() {
        let sock=fixture("hud-sock").join("h.sock");
        let _=fs::remove_file(&sock);
        let listener=std::os::unix::net::UnixListener::bind(&sock).unwrap();
        let server=thread::spawn(move||{
            let mut methods=Vec::new();
            for _ in 0..2 {
                let (stream,_)=listener.accept().unwrap(); let mut line=String::new();
                BufReader::new(&stream).read_line(&mut line).unwrap();
                let v:Value=serde_json::from_str(&line).unwrap(); let m=text(&v,"method").to_string();
                let reply=if m=="pane.get" {json!({"id":"hud","result":{"pane":{"pane_id":"w1:p1","workspace_id":"w1"}}})}
                    else {json!({"id":"hud","result":{"workspace":{"workspace_id":"w1","label":"gs3 · repo"}}})};
                writeln!(&stream,"{reply}").unwrap(); methods.push((m,v["params"].clone()));
            }
            methods
        });
        let p=pane(&sock,"w1:p1").unwrap().unwrap();
        assert_eq!(text(&p,"workspace_label"),"gs3 · repo");
        let methods=server.join().unwrap(); fs::remove_file(&sock).unwrap();
        assert_eq!(methods,vec![("pane.get".to_string(),json!({"pane_id":"w1:p1"})),("workspace.get".to_string(),json!({"workspace_id":"w1"}))]);
    }
    #[test]
    fn sibling_creation_requeries_bound_pane_and_resets_session() {
        let dir=fixture("follow");
        fs::create_dir_all(&dir).unwrap();
        let old=dir.join("old.jsonl"); let new=dir.join("new.jsonl");
        for (path,input) in [(&old,99),(&new,7)] {fs::write(path,format!("{}\n",json!({"type":"message","message":{"role":"assistant","usage":{"input":input}}}))).unwrap();}
        let socket=dir.join("herdr.sock");
        let listener=std::os::unix::net::UnixListener::bind(&socket).unwrap();
        let next=new.clone();
        let server=thread::spawn(move||{
            for method in ["pane.get","workspace.get"] {
                let (stream,_)=listener.accept().unwrap(); let mut line=String::new();
                BufReader::new(&stream).read_line(&mut line).unwrap();
                let req:Value=serde_json::from_str(&line).unwrap(); assert_eq!(req["method"],method);
                let result=if method=="pane.get" {assert_eq!(req["params"]["pane_id"],"bound"); json!({"pane":{"pane_id":"bound","workspace_id":"w","agent_session":{"value":next}}})} else {json!({"workspace":{"label":"Probe"}})};
                writeln!(&stream,"{}",json!({"result":result})).unwrap();
            }
        });
        let (tx,rx)=mpsc::channel();
        let local=local(dir.clone(),socket,"bound".into(),tx);
        local.send(Local::Session(Some(old.to_string_lossy().into_owned()))).unwrap();
        let mut state=State::new("bound".into(),dir.clone()); state.session_path=Some(old.to_string_lossy().into_owned());
        apply(&mut state,rx.recv_timeout(Duration::from_secs(3)).unwrap()); assert_eq!(state.session.input,99);
        // Binding also publishes Stack before later filesystem notifications.
        match rx.recv_timeout(Duration::from_secs(3)).unwrap() {
            Event::Stack(tools)=>{assert_eq!(tools.len(),6); assert_eq!(tools[1].version.as_deref(),Some(env!("CARGO_PKG_VERSION")));},
            _=>panic!("expected Stack after session binding"),
        }
        local.send(Local::Changed(notify::Event{kind:notify::EventKind::Create(notify::event::CreateKind::File),paths:vec![new.clone()],attrs:Default::default()})).unwrap();
        apply(&mut state,rx.recv_timeout(Duration::from_secs(3)).unwrap());
        assert_eq!(state.session_path.as_deref(),new.to_str()); assert_eq!(state.session.input,0); assert!(state.read_session);
        assert_eq!(state.label,"Probe");
        local.send(Local::Session(state.session_path.clone())).unwrap();
        apply(&mut state,rx.recv_timeout(Duration::from_secs(3)).unwrap()); assert_eq!(state.session.input,7);
        server.join().unwrap(); drop(local); fs::remove_dir_all(&dir).unwrap();
    }
    #[test]
    fn context_window_prefers_runtime_model_cache() {
        let agent=fixture("limit"); let dir=agent.join("sessions/--x--");
        fs::create_dir_all(&dir).unwrap();
        let session=dir.join("s.jsonl");
        fs::write(&session,format!("{}\n",json!({"type":"message","message":{"role":"assistant","model":"m","usage":{"totalTokens":10}}}))).unwrap();
        // Pi registers cpamc models from the cache; the store is only a fallback.
        fs::write(agent.join("cpamc-models-cache.json"),json!([{"id":"m","contextWindow":272000}]).to_string()).unwrap();
        fs::write(agent.join("models-store.json"),json!({"cpamc":{"models":[{"id":"m","contextWindow":370000}]}}).to_string()).unwrap();
        let mut t=Tail::default(); read_tail(&session,&mut t).unwrap(); assert_eq!(t.session.limit,Some(272000));
        fs::remove_file(agent.join("cpamc-models-cache.json")).unwrap();
        let mut t=Tail::default(); read_tail(&session,&mut t).unwrap(); assert_eq!(t.session.limit,Some(370000));
        fs::remove_dir_all(&agent).unwrap();
    }
    #[test]
    fn bundled_store_is_derived_only_when_it_exists() {
        let dir=fixture("bundled"); 
        let agent=dir.join("prefix/agent"); fs::create_dir_all(&agent).unwrap();
        // A custom home is not read from a guessed store that does not exist.
        assert_eq!(bundled_models(&agent),None);
        fs::create_dir_all(dir.join("prefix/config")).unwrap();
        fs::write(dir.join("prefix/config/models.json"),"{}").unwrap();
        assert_eq!(bundled_models(&agent),Some(dir.join("prefix/config/models.json")));
        assert_eq!(bundled_models(&dir),None,"a prefix without its own agent home is not a bundle");
        fs::remove_dir_all(&dir).unwrap();
    }
    #[test]
    fn git_snapshot_and_detail_read_a_real_repo() {
        let dir=fixture("git");  fs::create_dir_all(dir.join("sub")).unwrap();
        let sh=|c:&str|{let o=Command::new("/bin/sh").args(["-c",c]).current_dir(&dir).output().unwrap(); assert!(o.status.success(),"{c}: {}",String::from_utf8_lossy(&o.stderr));};
        sh("git init -q -b main && git config user.email t@t && git config user.name t && printf 'a\\nb\\n' > f.txt && git add f.txt && git commit -qm base");
        sh("git checkout -qb feat && printf 'x\\n' > g.txt && git add g.txt && git commit -qm side && git checkout -q main");
        sh("printf 'a\\nc\\n' > f.txt && printf 'new\\n' > sub/n.txt && printf 'S=1\\n' > .env && ln -s /etc/hostname link");
        let g=git_snapshot(&dir.join("sub"));
        // Pages: one commit asked for is one commit read; the ceiling clamps a silly number.
        let one=git_log(&dir,1); assert_eq!((one.commits.len(),one.log_limit),(1,1),"una página de uno");
        assert_eq!(git_log(&dir,10_000).log_limit,LOG_MAX,"el techo manda");
        assert_eq!(g.error,None); assert_eq!((g.counts.modified,g.counts.untracked),(1,3)); assert_eq!((g.added,g.deleted),(1,1));
        assert!(g.files.iter().any(|f|f.path=="sub/n.txt"),"root-relative paths from a subdirectory: {:?}",g.files);
        assert!(g.graph[0].contains("(HEAD -> main)") && g.commits.len()==1 && g.commits[0].ends_with(" base"),"{:?}",g.graph);
        assert_eq!(g.branches.iter().map(|b|(b.name.as_str(),b.current)).collect::<Vec<_>>(),[("main",true),("feat",false)]);
        let req=|b:Option<&str>,f:Option<&str>,u|DetailRequest{branch:b.map(Into::into),file:f.map(Into::into),commit:None,untracked:u};
        let d=detail(&dir.join("sub"),&req(None,Some("f.txt"),false)); assert!(d.diff.iter().any(|l|l=="-b") && d.diff.iter().any(|l|l=="+c"),"{:?}",d.diff);
        assert_eq!(detail(&dir,&req(None,Some("sub/n.txt"),true)).diff,["@@ archivo nuevo @@","+new"]);
        assert!(detail(&dir,&req(None,Some(".env"),true)).diff.is_empty(),"secrets are never read");
        assert_eq!(detail(&dir,&req(None,Some("link"),true)).diff,["enlace simbólico (no se sigue)"]);
        let d=detail(&dir,&req(Some("feat"),None,false)); assert_eq!((d.files.len(),d.added,d.error.clone()),(1,1,None)); assert_eq!(d.files[0].path,"g.txt");
        let d=detail(&dir,&req(Some("feat"),Some("g.txt"),false)); assert!(d.diff.contains(&"+x".to_string()));
        assert!(detail(&dir,&req(Some("--output=/tmp/x"),None,false)).error.is_some(),"branch names are never options");
        // A commit carries its own words to the window: author, when, and the whole message.
        let hash=g.commits[0].split_whitespace().next().unwrap().to_string();
        let d=detail(&dir,&DetailRequest{branch:None,file:None,commit:Some(hash),untracked:false});
        assert_eq!((d.author.as_str(),d.message.trim()),(("t"),"base"),"autor y mensaje del commit");
        assert!(!d.date.is_empty(),"y cuándo fue");
        fs::remove_dir_all(&dir).unwrap();
    }
    #[test]
    fn tree_scan_lists_dirs_first_and_only_opens_requested_dirs() {
        let dir=fixture("tree"); 
        for d in ["src/inner","z/node_modules/pkg","target/release"] {fs::create_dir_all(dir.join(d)).unwrap();}
        // A real repository boundary keeps this fixture isolated even when TMPDIR sits inside another repo.
        assert!(Command::new("git").arg("init").arg("-q").arg(&dir).status().unwrap().success());
        for f in ["b.txt","A.md","src/m.rs","src/inner/deep.rs","z/y.rs","target/release/bin"] {fs::write(dir.join(f),"x").unwrap();}
        std::os::unix::fs::symlink(dir.join("src"),dir.join("link")).unwrap();
        let t=tree(&dir,vec!["src".into(),"src/inner".into()]);
        let names=|n:&[TreeNode]|n.iter().map(|n|(n.name.clone(),n.dir,n.count)).collect::<Vec<_>>();
        assert_eq!(names(&t.nodes),[("src".into(),true,2),("z".into(),true,1),("A.md".into(),false,0),("b.txt".into(),false,0),("link".into(),false,0)]);
        assert_eq!(names(&t.nodes[0].children),[("inner".into(),true,1),("m.rs".into(),false,0)]);
        assert_eq!(t.nodes[0].children[0].children[0].rel,"src/inner/deep.rs"); assert!(t.nodes[1].children.is_empty(),"closed dirs are counted, not scanned");
        assert_eq!((t.project.as_str(),t.open.len(),t.error.is_none()),(dir.file_name().unwrap().to_str().unwrap(),2,true));
        // A child opened under a closed parent is not scanned; a missing root is an error.
        assert!(tree(&dir,vec!["src/inner".into()]).nodes[0].children.is_empty());
        assert!(tree(&dir.join("missing"),vec![]).error.is_some());
        fs::remove_dir_all(&dir).unwrap();
    }
    #[test]
    fn preview_reads_text_and_refuses_what_must_never_show() {
        let dir=fixture("preview"); 
        for d in ["src","assets","docs"] {fs::create_dir_all(dir.join(d)).unwrap();}
        fs::write(dir.join("src/main.rs"),"fn main() {}\n").unwrap();
        fs::write(dir.join("nota.txt"),"hola\n").unwrap();
        fs::write(dir.join("assets/logo.png"),[0x89u8,0x50,0x4e,0x47]).unwrap();
        fs::write(dir.join("docs/guia.md"),"# Guía\n\ntexto\n").unwrap();
        fs::write(dir.join("app.bin"),[0u8,1,2,3]).unwrap();
        fs::write(dir.join("secrets.yml"),"k: v\n").unwrap();
        fs::write(dir.join(".env"),"TOKEN=1\n").unwrap();
        std::os::unix::fs::symlink(dir.join("src/main.rs"),dir.join("link.rs")).unwrap();
        // A source file arrives as code, with the grammar that colours it; a .txt stays plain text.
        assert_eq!(preview(&dir,"src/main.rs"),Preview::Code{path:"src/main.rs".into(),body:"fn main() {}\n".into(),cut:false,lang:"Rust".into()});
        assert_eq!(preview(&dir,"nota.txt"),Preview::Text{path:"nota.txt".into(),body:"hola\n".into(),cut:false});
        // An ESC in the file is data: the window must never read it as colour.
        fs::write(dir.join("raw.txt"),"antes \u{1b}[31mrojo\u{1b}[0m\n").unwrap();
        let Preview::Text{body,..}=preview(&dir,"raw.txt") else {panic!("raw.txt no es texto")};
        assert!(!body.contains('\u{1b}') && body.contains('\u{241b}'), "el ESC se muestra como símbolo: {body:?}");
        // An image is never read here: the path travels and the window draws it.
        assert_eq!(preview(&dir,"assets/logo.png"),Preview::Image{path:"assets/logo.png".into(),file:dir.join("assets/logo.png")});
        // Markdown is read as text but announced as such, so the window renders it instead of showing source.
        assert_eq!(preview(&dir,"docs/guia.md"),Preview::Markdown{path:"docs/guia.md".into(),body:"# Guía\n\ntexto\n".into(),cut:false});
        for p in ["secrets.yml",".env","link.rs","src","../outside","/etc/passwd","src//main.rs","missing.rs"] {
            assert!(matches!(preview(&dir,p),Preview::Refused{..}),"{p} se mostro");
        }
        assert!(matches!(preview(&dir,"app.bin"),Preview::Refused{..}),"un binario no se muestra");
        // An image too big to decode is refused instead of read into memory.
        let big=fs::File::create(dir.join("huge.png")).unwrap(); big.set_len(IMAGE_MAX+1).unwrap(); drop(big);
        assert!(matches!(preview(&dir,"huge.png"),Preview::Refused{..}),"una imagen de 32 MiB o mas no se abre");
        // A long text file is cut at the ceiling and says so instead of being refused.
        fs::write(dir.join("big.txt"),"x".repeat(PREVIEW_MAX as usize+10)).unwrap();
        let Preview::Text{body,cut,..}=preview(&dir,"big.txt") else {panic!("big.txt no es texto")};
        assert!(cut && body.len()==PREVIEW_MAX as usize);
        // So is a long markdown file, which is still markdown after the cut.
        fs::write(dir.join("docs/big.md"),"# t\n".repeat(PREVIEW_MAX as usize/4+4)).unwrap();
        let Preview::Markdown{body,cut,..}=preview(&dir,"docs/big.md") else {panic!("big.md no es markdown")};
        assert!(cut && body.len()==PREVIEW_MAX as usize);
        fs::remove_dir_all(&dir).unwrap();
    }
    #[test]
    fn a_task_file_updates_the_roster_without_pressing_r() {
        let root=fixture("roster"); 
        let agent=root.join("agent"); let sessions=agent.join("sessions").join("proj"); let tasks=agent.join("gentle-agents").join("tasks");
        fs::create_dir_all(&sessions).unwrap(); fs::create_dir_all(&tasks).unwrap();
        let session=sessions.join("2026-01-01T00-00-00-000Z_0d0d0d0d-0d0d-0d0d-0d0d-0d0d0d0d0d0d.jsonl"); fs::write(&session,"").unwrap();
        let (tx,rx)=mpsc::channel();
        let local=local(root.clone(),std::env::temp_dir().join("gs3-roster.sock"),"p".into(),tx);
        let _=local.send(Local::Session(Some(session.to_string_lossy().into_owned())));
        // Gentle AI writes the task when it launches it: the roster must hear about it on its own.
        fs::write(tasks.join("t1.json"),json!({"task":{"agent":"gentle-ai-worker","label":"corriendo","status":"running","parentSessionId":"0d0d0d0d-0d0d-0d0d-0d0d-0d0d0d0d0d0d"}}).to_string()).unwrap();
        let deadline=Instant::now()+Duration::from_secs(5);
        let mut alive:Option<Roster>=None;
        while Instant::now()<deadline {
            if let Ok(Event::Roster(Some(r)))=rx.recv_timeout(Duration::from_millis(200)) { if r.active==1 {alive=Some(r); break} }
        }
        let r=alive.expect("un archivo de tarea nuevo tiene que llegar por el watcher, sin apretar r");
        assert_eq!(r.running.iter().map(|t|t.text.as_str()).collect::<Vec<_>>(),["gentle-ai-worker · corriendo"],"y decir cuál está viva");
        let _=fs::remove_dir_all(&root);
    }
    #[test]
    fn the_roster_counts_only_this_session_and_its_subagents_tasks() {
        let root=fixture("sess"); 
        let agent=root.join("agent"); let sessions=agent.join("sessions").join("proj"); let tasks=agent.join("gentle-agents").join("tasks");
        let sub_sessions=agent.join("gentle-agents").join("sessions");
        fs::create_dir_all(&sessions).unwrap(); fs::create_dir_all(&tasks).unwrap(); fs::create_dir_all(&sub_sessions).unwrap();
        let mine="0a0a0a0a-0a0a-0a0a-0a0a-0a0a0a0a0a0a"; let other="9b9b9b9b-9b9b-9b9b-9b9b-9b9b9b9b9b9b";
        let sub="5c5c5c5c-5c5c-5c5c-5c5c-5c5c5c5c5c5c";
        let log=|id:&str|sessions.join(format!("2026-01-01T00-00-00-000Z_{id}.jsonl"));
        fs::write(log(mine),"").unwrap(); fs::write(log(other),"").unwrap();
        let sub_log=sub_sessions.join(format!("2026-01-01T00-00-00-001Z_{sub}.jsonl")); fs::write(&sub_log,"").unwrap();
        let task=|name:&str,v:Value|fs::write(tasks.join(name),v.to_string()).unwrap();
        task("a1.json",json!({"task":{"agent":"gentle-ai-worker","label":"hija","status":"completed","parentSessionId":mine,"sessionPath":sub_log.to_string_lossy(),"createdAt":1}}));
        task("a2.json",json!({"task":{"agent":"gentle-ai-worker","label":"nieta","status":"failed","parentSessionId":sub,"createdAt":2}}));
        task("b1.json",json!({"task":{"agent":"gentle-ai-worker","label":"ajena","status":"running","parentSessionId":other,"createdAt":3}}));
        let r=roster_of(&log(mine),&root).unwrap();
        assert_eq!((r.total,r.done,r.failed,r.active),(2,1,1,0),"sólo las tareas de esta sesión y de sus subagentes, no las de otra sesión");
        let r=roster_of(&log(other),&root).unwrap();
        assert_eq!((r.total,r.active),(1,1),"la otra sesión ve sólo la suya");
        let _=fs::remove_dir_all(&root);
    }
    #[test]
    fn only_paths_inside_the_task_history_count() {
        let dir=PathBuf::from("/a/agent");
        let event=|p:&str|notify::Event{kind:notify::EventKind::Create(notify::event::CreateKind::File),paths:vec![PathBuf::from(p)],attrs:Default::default()};
        assert!(is_task_file(&event("/a/agent/gentle-agents/tasks/t1.json"),&dir));
        assert!(!is_task_file(&event("/a/agent/sessions/p/s.jsonl"),&dir));
        assert!(!is_task_file(&event("/a/agent/gentle-agents/tasks-otra/t1.json"),&dir),"no es la carpeta de tareas");
    }
    #[test]
    fn profiles_store_reads_a_fixture_and_refuses_links_and_garbage() {
        let dir=fixture("profstore");
         fs::create_dir_all(&dir).unwrap();
        // The store the package really writes: kind, version, profiles by name, and a global active.
        let store=json!({"kind":"gentle-pi.agent_model_profiles","version":1,"active":"b",
            "profiles":{"a":{"orchestrator":{"model":"m1"}},"b":{"worker":"m2"}}});
        fs::write(dir.join("profiles.json"),store.to_string()).unwrap();
        let read=profiles_store(Some(dir.join("profiles.json"))).unwrap();
        assert_eq!(profiles(Some(&read)).iter().map(|p|(p.name.as_str(),p.active)).collect::<Vec<_>>(),[("a",false),("b",true)]);
        assert_eq!(profiles_store(None),None,"ruta desconocida");
        assert_eq!(profiles_store(Some(dir.join("missing.json"))),None,"archivo ausente");
        fs::write(dir.join("array.json"),"[]").unwrap();
        assert_eq!(profiles_store(Some(dir.join("array.json"))),None,"un array no es el store");
        fs::write(dir.join("broken.json"),"{").unwrap();
        assert_eq!(profiles_store(Some(dir.join("broken.json"))),None,"JSON roto");
        std::os::unix::fs::symlink(dir.join("profiles.json"),dir.join("link.json")).unwrap();
        assert_eq!(profiles_store(Some(dir.join("link.json"))),None,"nunca se sigue un enlace simbólico");
        fs::remove_dir_all(&dir).unwrap();
    }
    #[test]
    fn only_the_profiles_store_file_counts() {
        let dir=PathBuf::from("/cfg");
        let event=|p:&str|notify::Event{kind:notify::EventKind::Create(notify::event::CreateKind::File),paths:vec![PathBuf::from(p)],attrs:Default::default()};
        assert!(is_profiles_file(&event("/cfg/profiles.json"),&dir));
        // The package swaps the store through a temp sibling: that rename is the real signal.
        assert!(is_profiles_file(&event("/cfg/.profiles.json.abc.tmp"),&dir));
        assert!(!is_profiles_file(&event("/cfg/models.json"),&dir));
        assert!(!is_profiles_file(&event("/cfg/profiles.export.json"),&dir));
        assert!(!is_profiles_file(&event("/sub/profiles.json"),&dir),"sólo la carpeta del store");
    }
    #[test]
    fn only_profile_pin_writes_count() {
        let e=|p:&str|notify::Event{kind:notify::EventKind::Create(notify::event::CreateKind::File),paths:vec![PathBuf::from(p)],attrs:Default::default()};
        let pinned=is_profile_pin_file;
        // Both layers are swapped through a temp sibling and a rename.
        assert!(pinned(&e("/repo/.git/gentle-ai/profile-pin.json")));
        assert!(pinned(&e("/repo/.git/gentle-ai/.profile-pin.json.x.tmp")));
        assert!(pinned(&e("/repo/.pi/gentle-ai/profile.json")));
        assert!(pinned(&e("/repo/.pi/gentle-ai/.profile.json.x.tmp")));
        assert!(!pinned(&e("/repo/.pi/gentle-ai/profiles.json")));
        assert!(!pinned(&e("/repo/gentle-ai/profile-pin.json")),"fuera de las dos capas no cuenta");
    }
    #[test]
    fn a_pin_file_updates_the_roster_without_pressing_r() {
        let root=fixture("pinwatch");
        let agent=root.join("agent"); let sessions=agent.join("sessions").join("proj");
        fs::create_dir_all(&sessions).unwrap(); fs::create_dir_all(root.join(".git/gentle-ai")).unwrap(); fs::create_dir_all(root.join(".pi/gentle-ai")).unwrap();
        let session=sessions.join("s.jsonl"); fs::write(&session,"").unwrap();
        assert!(Command::new("git").arg("init").arg("-q").arg(&root).status().unwrap().success(),"el pin vive en un repo");
        // The store the pin names must exist in, or the pin is stale by definition.
        let cfg=root.join("cfg"); fs::create_dir_all(&cfg).unwrap();
        fs::write(cfg.join("models.json"),"{}").unwrap();
        fs::write(cfg.join("profiles.json"),json!({"kind":"gentle-pi.agent_model_profiles","version":1,"profiles":{"uno":{"worker":"m"},"dos":{"worker":"m"}}}).to_string()).unwrap();
        let (tx,rx)=mpsc::channel();
        let local=local(root.clone(),std::env::temp_dir().join("gs3-pinwatch.sock"),"p".into(),tx);
        std::env::set_var("GENTLE_PI_AGENT_HOME",&agent); std::env::set_var("GENTLE_PI_CONFIG_HOME",&cfg);
        let restore=||{std::env::remove_var("GENTLE_PI_AGENT_HOME"); std::env::remove_var("GENTLE_PI_CONFIG_HOME");};
        let _=local.send(Local::Session(Some(session.to_string_lossy().into_owned())));
        // Baseline roster first: the watcher only sends what changed.
        let deadline=Instant::now()+Duration::from_secs(5);
        while Instant::now()<deadline { if let Ok(Event::Roster(_))=rx.recv_timeout(Duration::from_millis(200)) {break} }
        let pin=json!({"kind":"gentle-pi.agent_model_profile_pin","version":1,"profile":"uno"});
        // Pi writes the repo declaration through a temp sibling and a rename.
        let tmp=root.join(".pi/gentle-ai/.profile.json.probe.tmp");
        fs::write(&tmp,pin.to_string()).unwrap(); fs::rename(&tmp,root.join(".pi/gentle-ai/profile.json")).unwrap();
        let mut seen:Option<Roster>=None;
        while Instant::now()<deadline {
            if let Ok(Event::Roster(Some(r)))=rx.recv_timeout(Duration::from_millis(200)) { if r.pinned.as_deref()==Some("uno") {seen=Some(r); break} }
        }
        restore(); drop(local);
        let r=seen.expect("un pin nuevo tiene que llegar por el watcher, sin apretar r");
        assert_eq!(r.pinned.as_deref(),Some("uno"));
        let _=fs::remove_dir_all(&root);
    }
    #[test]
    fn engram_connection_is_read_only() {
        let path=std::env::temp_dir().join(format!("gs3-hud-readonly-{}.db",std::process::id()));
        {let db=Connection::open(&path).unwrap(); db.execute_batch("CREATE TABLE observations(project TEXT, deleted_at TEXT); INSERT INTO observations VALUES('test',NULL);").unwrap();}
        let db=open_engram(&path).unwrap();
        assert_eq!(db.query_row("SELECT count(*) FROM observations",[],|r|r.get::<_,i64>(0)).unwrap(),1);
        assert!(db.execute("INSERT INTO observations VALUES('bad',NULL)",[]).is_err());
        drop(db); fs::remove_file(path).unwrap();
    }
}

