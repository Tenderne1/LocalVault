import React,{useEffect,useLayoutEffect,useMemo,useRef,useState} from "react";
import {createRoot} from "react-dom/client";
import {invoke} from "@tauri-apps/api/core";
import {listen} from "@tauri-apps/api/event";
import {open,save} from "@tauri-apps/plugin-dialog";
import {check,type Update} from "@tauri-apps/plugin-updater";
import {openUrl} from "@tauri-apps/plugin-opener";
import {getCurrentWindow} from "@tauri-apps/api/window";
import {enable as autostartEnable,disable as autostartDisable,isEnabled as autostartIsEnabled} from "@tauri-apps/plugin-autostart";
import {register as gsRegister,unregister as gsUnregister} from "@tauri-apps/plugin-global-shortcut";
import "./styles.css";

type Category={name:string;icon:string;parentName:string|null};
type Entry={id:string;seq:number;type:string;name:string;username:string;email:string;phone:string;password:string;nickname:string;url:string;notes:string;category:string;tags:string[];favorite:boolean;updatedAt:number;expiresAt:number|null;passwordEncrypted?:string|null;passwordScore?:number;passwordReused?:boolean};
type History={changedAt:number;fields:string[]};
type Status={exists:boolean;version:number;recoveryEnabled:boolean};
type BackupData={entries:Entry[];categories:Category[]};
type BackupSettings={enabled:boolean;directory:string|null;retention:number;lastBackupAt:number|null;lastError:string|null};
type BrowserChoice={useDefault:boolean;path:string};
type BrowserInfo={path:string;name:string;icon:string|null};
type DiagnosticsSettings={enabled:boolean};
type UnlockSecurityState={failedAttempts:number;captchaRequired:boolean;lockUntil:number|null;captchaSvg:string|null};
type View="loading"|"create"|"recoverySetup"|"unlock"|"recoveryUnlock"|"recoveryReset"|"vault"|"about";
type Dialog="none"|"category"|"delete"|"discard"|"history"|"import"|"importMethod"|"trash"|"categoryEdit"|"credential"|"backupRestore"|"backupSettings"|"securitySettings"|"update"|"bulkImport"|"generator"|"browserSettings"|"prompt"|"recoveryCode"|"shortcuts"|"autofill"|"generalSettings";
type ClosePref={action:"tray"|"quit";remember:boolean};

const APP_VERSION="1.9.3";
type UpdateVer={tag:string;date:string;body:string;assets:{name:string;url:string}[]};
const getInitialTheme=(): "light"|"dark"=>{try{const t=localStorage.getItem("lv_theme");if(t==="light"||t==="dark")return t;if(window.matchMedia&&window.matchMedia("(prefers-color-scheme: dark)").matches)return "dark"}catch{}return "light"};
(()=>{try{document.documentElement.dataset.theme=getInitialTheme()}catch{}})();
const versionGt=(a:string,b:string)=>{const p=(v:string)=>{const m=(v.replace(/^v/i,"").split(".").map(x=>parseInt(x,10)||0));while(m.length<3)m.push(0);return m};const x=p(a),y=p(b);for(let i=0;i<3;i++){if(x[i]!==y[i])return x[i]>y[i]}return false};
type AutofillStatus={enabled:boolean;running:boolean;port:number;vaultUnlocked:boolean;pairedCount:number;lastError:string|null};
const RELEASE_NOTES:string[]=[
 "新增：浏览器自动填充（🌐 浏览器填充）——本地回环 HTTP + MV3 扩展，首次配对后解锁即可填充。",
 "新增：全局唤出快捷键（默认 Ctrl+Alt+L）、开机自启、点 × 可选择最小化到系统托盘。",
 "新增：浅色/暗色主题切换；软件更新支持按版本查看说明与下载直链。",
 "安全：解锁后前端不再持有任何密码明文（内存会话密钥单条加密）。",
 "修复：浏览器扩展后台不可用、部分网站 Tab 键跳转异常、锁定后再解锁扩展失效。"
];
const TYPES=["平台","网站","系统","APP","其他"];
const DEFAULT_CATS:Category[]=[{name:"默认",icon:"▦",parentName:null},{name:"工作",icon:"💼",parentName:null},{name:"财务",icon:"💰",parentName:null},{name:"社交",icon:"👥",parentName:null},{name:"开发",icon:"💻",parentName:null},{name:"其他",icon:"📁",parentName:null}];
function scheduleClipboardClear(){setTimeout(()=>{invoke("clipboard_clear").catch(()=>{})},30000)}
async function writeSecure(text:string){try{await invoke("copy_secure",{text});return true}catch{return false}}
async function copyText(text:string,secure=false){let ok=secure?await writeSecure(text):false;if(!ok){try{await navigator.clipboard.writeText(text);ok=true}catch{try{const ta=document.createElement("textarea");ta.value=text;ta.style.position="fixed";ta.style.opacity="0";document.body.appendChild(ta);ta.select();ok=document.execCommand("copy");ta.remove()}catch{ok=false}}}if(ok&&secure){scheduleClipboardClear()}return ok}
const now=()=>Date.now(),uid=()=>crypto.randomUUID();
function validMasterPassword(p:string){return p.length>=8&&/[a-z]/.test(p)&&/[A-Z]/.test(p)&&/[0-9]/.test(p)&&/[^A-Za-z0-9]/.test(p)}
function score(p:string){if(!p)return 0;let s=Math.min(50,p.length*2);if(/[a-z]/.test(p))s+=8;if(/[A-Z]/.test(p))s+=8;if(/[0-9]/.test(p))s+=8;if(/[^A-Za-z0-9]/.test(p))s+=8;if(/^(password|123456|qwerty|admin|letmein|welcome)/i.test(p))s=Math.min(s,12);if(/(.)\1{3,}/.test(p))s-=15;if(/1234|abcd|qwerty|asdf|zxcv/i.test(p))s-=12;return Math.max(0,Math.min(100,s))}
const DEFAULT_SHORTCUTS={search:"Control+f",save:"Control+s",lock:"Control+l"};
const DEFAULT_GLOBAL_HOTKEY="Control+Alt+l";
const parseKey=(e:KeyboardEvent)=>{const mods:string[]=[];if(e.ctrlKey||e.metaKey)mods.push("Control");if(e.altKey)mods.push("Alt");if(e.shiftKey)mods.push("Shift");const k=e.key.toLowerCase();if(k==="control"||k==="alt"||k==="shift"||k==="meta")return "";return [...mods,k].join("+")};
const shortcutLabel=(k:"search"|"save"|"lock")=>k==="search"?"搜索":k==="save"?"保存":"锁定";
const CATEGORY_ICONS=[
 {group:"常用",icons:["📁","📂","🗂️","📚","💼","🏢","💰","💳","🏦","👥","🧑‍💼","🔐","🔑","🗝️","📧","📱","💻","🖥️","🌐","🔗"]},
 {group:"生活",icons:["🏠","🚗","✈️","🎒","🍔","☕","🛒","🎮","🎵","🎬","📺","🎨","📷","🎧","⚽","🏋️","🧘","🐶","🌿","💊","🩺","🎓"]},
 {group:"其它",icons:["⭐","🔥","⚡","💡","🎯","📌","🏷️","📦","⚙️","🧩","🪪","🧠","🤖","🌈","🌙","☀️","❄️","🍀","🎁","📝","📄","🖊️"]}
];
function emptyEntry(seq:number,category:string):Entry{return{id:uid(),seq,type:"网站",name:"新密码",username:"",email:"",phone:"",password:"",nickname:"",url:"",notes:"",category,tags:[],favorite:false,updatedAt:now(),expiresAt:null,passwordEncrypted:null,passwordScore:0,passwordReused:false}}
function dateInput(ts:number|null){return ts?new Date(ts).toISOString().slice(0,10):""}
function expiryDate(days:number){return now()+days*86400000}
function parseCsv(text:string){
 const rows:string[][]=[];let row:string[]=[],cell="",quoted=false;
 const src=text.replace(/^\uFEFF/,"");
 for(let i=0;i<src.length;i++){const ch=src[i];if(quoted){if(ch==='"'){if(src[i+1]==='"'){cell+='"';i++}else quoted=false}else cell+=ch}else if(ch==='"'){quoted=true}else if(ch===','){row.push(cell);cell=""}else if(ch==='\n'){row.push(cell);rows.push(row);row=[];cell=""}else if(ch==='\r'){if(src[i+1]==='\n')i++;row.push(cell);rows.push(row);row=[];cell=""}else cell+=ch; if(cell.length>4096)throw new Error("CSV 单元格过长：单个字段最多 4096 个字符")}
 if(quoted)throw new Error("CSV 格式错误：存在未闭合的引号");
 if(cell!==""||row.length){row.push(cell);rows.push(row)}
 if(rows.length>100001)throw new Error("CSV 行数过多：最多允许 100000 条记录");
 return rows;
}
function parseBulkEntries(text:string){
 const rows=parseCsv(text);if(!rows.length)throw new Error("模板为空");
 const headers=rows[0].map(x=>x.trim());const idx=new Map(headers.map((h,i)=>[h,i]));
 const missing=["名称","账号/用户名"].filter(h=>!idx.has(h));if(missing.length)throw new Error(`模板缺少必要列：${missing.join("、")}`);
 const out:Entry[]=[];let demoRowsFound=0;
 for(let i=1;i<rows.length;i++){
  const r=rows[i];if(r.every(x=>!x.trim()))continue;
  const get=(h:string)=>r[idx.get(h)??-1]??"";
  const raw=["名称","账号/用户名","密码","网址/IP/APP名称","账号类型","分类","邮箱","手机号","平台昵称","标签","密码有效期","备注"].map(get);
  if(raw.some(x=>x.length>4096))throw new Error(`第 ${i+1} 行存在超长字段`);
  const name=get("名称").trim();if(!name)continue;
  const tags=get("标签").split(/[、,，]/).map(x=>x.trim()).filter(Boolean);
  const note=get("备注").trim();
  const isDemo=tags.includes("LocalVault示例")||note.includes("【LocalVault模板示例，不会导入】")||
   (name==="示例：GitHub"&&get("账号/用户名").trim()==="demo@example.com"&&get("密码")==="Demo-GitHub-2026!")||
   (name==="示例：企业邮箱"&&get("账号/用户名").trim()==="demo@example.com"&&get("密码")==="Demo-Mail-2026!");
  if(isDemo){demoRowsFound++;continue}
  if(name.length>200)throw new Error(`第 ${i+1} 行：名称最多 200 个字符`);
  const password=get("密码");if(password.length>2048)throw new Error(`第 ${i+1} 行：密码最多 2048 个字符`);
  if(tags.length>100)throw new Error(`第 ${i+1} 行：标签过多`);
  const fav=["1","true","是","√","yes","y"].includes(get("收藏").trim().toLowerCase());
  const validity=get("密码有效期").trim().replace(/\s+/g,"");let expiresAt:number|null=null;if(validity&&validity!=="永不过期"){const validityDays:Record<string,number>={"30":30,"30天":30,"90":90,"90天":90,"180":180,"180天":180,"365":365,"365天":365};const days=validityDays[validity];if(!days)throw new Error(`第 ${i+1} 行：密码有效期无效，请使用“永不过期”“30天”“90天”“180天”或“365天”`);expiresAt=expiryDate(days)}
  out.push({id:uid(),seq:i,type:get("账号类型").trim()||"网站",name,username:get("账号/用户名").trim(),email:get("邮箱").trim(),phone:get("手机号").trim(),password,nickname:get("平台昵称").trim(),url:get("网址/IP/APP名称").trim(),notes:get("备注"),category:get("分类").trim()||"默认",tags,favorite:fav,updatedAt:now(),expiresAt});
 }
 return {entries:out,demoRowsFound};
}

function App(){
 const [view,setView]=useState<View>("loading"),[master,setMaster]=useState(""),[confirmMaster,setConfirmMaster]=useState("");
 const viewRef=useRef<View>(view);viewRef.current=view;
 const [unlockSecurity,setUnlockSecurity]=useState<UnlockSecurityState>({failedAttempts:0,captchaRequired:false,lockUntil:null,captchaSvg:null}),[captchaInput,setCaptchaInput]=useState(""),[unlockTick,setUnlockTick]=useState(0);
 const [entries,setEntries]=useState<Entry[]>([]),[categories,setCategories]=useState<Category[]>(DEFAULT_CATS),[selected,setSelected]=useState<string|null>(null);
 const [query,setQuery]=useState(""),[category,setCategory]=useState("全部"),[securityFilter,setSecurityFilter]=useState<"all"|"weak"|"reused"|"expired"|"expiring"|"untagged">("all"),[menu,setMenu]=useState<{x:number;y:number;kind:"blank"|"entry"|"category";id?:string;name?:string}|null>(null);
 const [dialog,setDialog]=useState<Dialog>("none"),[pendingAction,setPendingAction]=useState<(()=>void)|null>(null),[imported,setImported]=useState<Entry[]>([]),[bulkDemoStatus,setBulkDemoStatus]=useState(""),[conflictChoice,setConflictChoice]=useState<Record<string,"replace"|"skip">>({}),[categoryName,setCategoryName]=useState(""),[categoryIcon,setCategoryIcon]=useState("📁"),[categoryParent,setCategoryParent]=useState<string|null>(null),[error,setError]=useState(""),[detectedBrowsers,setDetectedBrowsers]=useState<BrowserInfo[]>([]),[browserUseDefaultDraft,setBrowserUseDefaultDraft]=useState(true),[browserPathDraft,setBrowserPathDraft]=useState("");
 const [autofill,setAutofill]=useState<AutofillStatus>({enabled:false,running:false,port:38527,vaultUnlocked:false,pairedCount:0,lastError:null}),[pairCode,setPairCode]=useState("");
 const [autoLock,setAutoLock]=useState(()=>{
   try{
     const value=Number(localStorage.getItem("lv_auto_lock")||5);
     return [1,5,10,30].includes(value)?value:5;
   }catch{return 5}
 }),[saveState,setSaveState]=useState("已保存");
 const [recoveryCode,setRecoveryCode]=useState(""),[generatedRecoveryCode,setGeneratedRecoveryCode]=useState("");
 const [questions,setQuestions]=useState(["我最喜欢的一本童年读物是什么？","我自己定义的长期不变短语是什么？","我记得的第一个特别地点是什么？"]),[answers,setAnswers]=useState(["","",""]);
 const [showPass,setShowPass]=useState(false),[revealed,setRevealed]=useState<{id:string;text:string}|null>(null),revealTimerRef=useRef<number|null>(null),[editMode,setEditMode]=useState(false),[draft,setDraft]=useState<Entry|null>(null),[multiSelected,setMultiSelected]=useState<string[]>([]),[history,setHistory]=useState<History[]>([]),[dirty,setDirty]=useState(false),[trash,setTrash]=useState<Entry[]>([]),[trashDays,setTrashDays]=useState(30),[categoryEdit,setCategoryEdit]=useState<Category|null>(null),[credentialPassword,setCredentialPassword]=useState(""),[pendingFile,setPendingFile]=useState<string|null>(null),[credentialKind,setCredentialKind]=useState<"backup"|"export"|"import"|"restore"|"settings"|"verify">("export"),[backupData,setBackupData]=useState<BackupData|null>(null);
 const [backupSettings,setBackupSettings]=useState<BackupSettings>({enabled:false,directory:null,retention:5,lastBackupAt:null,lastError:null});
 const [closeChoiceOpen,setCloseChoiceOpen]=useState(false),[closeChoiceAction,setCloseChoiceAction]=useState<"tray"|"quit">("tray"),[closeRemember,setCloseRemember]=useState(false),[closePref,setClosePref]=useState<ClosePref>({action:"tray",remember:false}),[autostart,setAutostart]=useState(false);
 const [globalHotkey,setGlobalHotkey]=useState<string>(DEFAULT_GLOBAL_HOTKEY),[globalHotkeyEnabled,setGlobalHotkeyEnabled]=useState(false),[globalHotkeyErr,setGlobalHotkeyErr]=useState("");
 const [backupEnabled,setBackupEnabled]=useState(false),[backupDirectory,setBackupDirectory]=useState(""),[backupRetention,setBackupRetention]=useState(5);
 const [browserChoice,setBrowserChoice]=useState<BrowserChoice>(()=>{try{return JSON.parse(localStorage.getItem("lv_browser_choice")||"null")||{useDefault:true,path:""}}catch{return {useDefault:true,path:""}}});
 const [securityQuestions,setSecurityQuestions]=useState(["我最喜欢的一本童年读物是什么？","我自己定义的长期不变短语是什么？","我记得的第一个特别地点是什么？"]),[securityAnswers,setSecurityAnswers]=useState(["","",""]),[changeMaster,setChangeMaster]=useState(false),[changeRecovery,setChangeRecovery]=useState(false),[newMaster,setNewMaster]=useState(""),[newConfirm,setNewConfirm]=useState(""),[verifiedMasterPassword,setVerifiedMasterPassword]=useState("");
 const [diagnosticsEnabled,setDiagnosticsEnabled]=useState(false);
 const [sortMode,setSortMode]=useState<"manual"|"name"|"updated">("manual"),[listCopied,setListCopied]=useState<string|null>(null),[generatorPick,setGeneratorPick]=useState<((pw:string)=>void)|null>(null);

 const [shortcuts,setShortcuts]=useState<{search:string;save:string;lock:string}>(()=>{try{const s=JSON.parse(localStorage.getItem("lv_shortcuts")||"null");return{...DEFAULT_SHORTCUTS,...(s||{})}}catch{return{...DEFAULT_SHORTCUTS}}});
 const [shortcutRecording,setShortcutRecording]=useState<null|"search"|"save"|"lock"|"global">(null);
 const searchRef=useRef<HTMLInputElement>(null);
 const menuRef=useRef<HTMLDivElement>(null);
 const [showOnboarding,setShowOnboarding]=useState(false);
 const [theme,setTheme]=useState<"light"|"dark">(getInitialTheme);
 useEffect(()=>{try{document.documentElement.dataset.theme=theme;localStorage.setItem("lv_theme",theme)}catch{}},[theme]);
 const [updateInfo,setUpdateInfo]=useState<Update|null>(null),[updateChecking,setUpdateChecking]=useState(false),[updateChecked,setUpdateChecked]=useState(false),[updateCheckedAt,setUpdateCheckedAt]=useState<number|null>(null),[updateInstalling,setUpdateInstalling]=useState(false),[updateConfirming,setUpdateConfirming]=useState(false),[updateProgress,setUpdateProgress]=useState(0),[updateTotal,setUpdateTotal]=useState(0),[updateError,setUpdateError]=useState(""),[updateVersions,setUpdateVersions]=useState<UpdateVer[]>([]),[expandedVer,setExpandedVer]=useState<string|null>(null),[promptMeta,setPromptMeta]=useState<{kind:"type"|"tag"|"batchtag";title:string;placeholder:string}|null>(null),[promptValue,setPromptValue]=useState(""),[newRecoveryCode,setNewRecoveryCode]=useState("");
 const [types,setTypes]=useState<string[]>(()=>{try{return JSON.parse(localStorage.getItem("lv_types")||"null")||TYPES}catch{return TYPES}}),[tags,setTags]=useState<string[]>(()=>{try{return JSON.parse(localStorage.getItem("lv_tags")||"null")||["工作","高频使用","财务相关"]}catch{return ["工作","高频使用","财务相关"]}});
 const [sidebarWidth,setSidebarWidth]=useState(()=>Math.max(180,Math.min(320,Number(localStorage.getItem("lv_sidebar_width")||220))));
 const [listWidth,setListWidth]=useState(()=>Math.max(280,Math.min(620,Number(localStorage.getItem("lv_list_width")||380))));
 const [draggedId,setDraggedId]=useState<string|null>(null),[dragOverId,setDragOverId]=useState<string|null>(null);
 const [draggedCategory,setDraggedCategory]=useState<string|null>(null),[dragOverCategory,setDragOverCategory]=useState<string|null>(null);
 const [resizing,setResizing]=useState<"sidebar"|"list"|null>(null),[collapsedCats,setCollapsedCats]=useState<string[]>(()=>{try{return JSON.parse(localStorage.getItem("lv_collapsed_categories")||"[]")}catch{return []}}),[dragPoint,setDragPoint]=useState<{x:number;y:number}|null>(null);
 const [collapsedGroups,setCollapsedGroups]=useState<Record<string,boolean>>(()=>{try{return JSON.parse(localStorage.getItem("lv_sidebar_groups")||'{"security":false,"data":false,"system":false}')}catch{return {security:false,data:false,system:false}}});
 const pointerDrag=useRef<{kind:"entry"|"category";id:string;startX:number;startY:number;active:boolean;didDrag:boolean}|null>(null);
 const suppressClick=useRef(false);
 const dragOverIdRef=useRef<string|null>(null),dragOverCategoryRef=useRef<string|null>(null);
 const entriesRef=useRef<Entry[]>([]),categoriesRef=useRef<Category[]>([]),dragChangedRef=useRef(false);
 const lastActivityRef=useRef(now());
 const autoLockingRef=useRef(false);
 useEffect(()=>{entriesRef.current=entries},[entries]);
 useEffect(()=>{categoriesRef.current=categories},[categories]);

 const refreshUnlockSecurity=async()=>{try{const s=await invoke<UnlockSecurityState>("vault_unlock_security_state");setUnlockSecurity(s);if(!s.captchaRequired)setCaptchaInput("")}catch{}};
 useEffect(()=>{if(view!=="unlock")return;void refreshUnlockSecurity();const id=setInterval(()=>setUnlockTick(Date.now()),1000);return()=>clearInterval(id)},[view]);
 useEffect(()=>{void loadDiagnostics();void loadAutofillStatus();(async()=>{try{const s=await invoke<Status>("vault_status");setView(s.exists?"unlock":"create")}catch(e){setError(String(e));setView("create")}})()},[]);
 useEffect(()=>{let un:(()=>void)|null=null;let cancelled=false;void listen("open-autofill-dialog",()=>{if(!cancelled)void openAutofillDialog()}).then(f=>{if(cancelled)f();else un=f});return()=>{cancelled=true;if(un)un()}},[]);
 useEffect(()=>{
   const markActivity=()=>{lastActivityRef.current=now()};
   const performAutoLock=async()=>{
     if(autoLockingRef.current)return;
     autoLockingRef.current=true;
     // 必须先让后端真正锁定（vault_lock 内部会先停填充服务、销毁 token），
     // 再切换前端界面，避免"界面已锁但服务/端口仍存活"的状态分裂。
     let ok=false;
     try{await invoke("vault_lock");ok=true}
     catch(e){setError("自动锁定失败，稍后自动重试："+String(e))}
     if(ok){
       setEntries([]);setSelected(null);setDraft(null);setEditMode(false);setDirty(false);setView("unlock");setMaster("");clearRevealed();
       void loadAutofillStatus();
       lastActivityRef.current=now();
     }else{
       // 后端未锁定时保持主界面并退避 30 秒再重试；不静默分裂
       lastActivityRef.current=now()+30000;
     }
     autoLockingRef.current=false;
   };
   const checkAutoLock=()=>{
     if(document.visibilityState==="hidden")return;
     if(view!=="vault"||autoLockingRef.current)return;
     if(now()-lastActivityRef.current>=autoLock*60000)void performAutoLock();
   };
   addEventListener("mousemove",markActivity,{passive:true});
   addEventListener("pointerdown",markActivity,{passive:true});
   addEventListener("keydown",markActivity);
   addEventListener("wheel",markActivity,{passive:true});
   addEventListener("touchstart",markActivity,{passive:true});
   addEventListener("focus",checkAutoLock);
   document.addEventListener("visibilitychange",checkAutoLock);
   const id=setInterval(checkAutoLock,1000);
   return()=>{
     removeEventListener("mousemove",markActivity);
     removeEventListener("pointerdown",markActivity);
     removeEventListener("keydown",markActivity);
     removeEventListener("wheel",markActivity);
     removeEventListener("touchstart",markActivity);
     removeEventListener("focus",checkAutoLock);
     document.removeEventListener("visibilitychange",checkAutoLock);
     clearInterval(id);
   };
 },[view,autoLock]);
 useEffect(()=>{localStorage.setItem("lv_auto_lock",String(autoLock))},[autoLock]);
 useEffect(()=>{localStorage.setItem("lv_sidebar_width",String(sidebarWidth))},[sidebarWidth]);
 useEffect(()=>{localStorage.setItem("lv_list_width",String(listWidth))},[listWidth]);
 useEffect(()=>{localStorage.setItem("lv_collapsed_categories",JSON.stringify(collapsedCats))},[collapsedCats]);
 useEffect(()=>{localStorage.setItem("lv_sidebar_groups",JSON.stringify(collapsedGroups))},[collapsedGroups]);
useEffect(()=>{localStorage.setItem("lv_shortcuts",JSON.stringify(shortcuts))},[shortcuts]);
 useEffect(()=>{if(!resizing)return;const move=(ev:MouseEvent)=>{if(resizing==="sidebar")setSidebarWidth(Math.max(180,Math.min(320,ev.clientX)));else setListWidth(Math.max(280,Math.min(620,ev.clientX-sidebarWidth)))};const up=()=>setResizing(null);addEventListener("mousemove",move);addEventListener("mouseup",up);return()=>{removeEventListener("mousemove",move);removeEventListener("mouseup",up)}},[resizing,sidebarWidth]);
 useEffect(()=>{
   const onKey=(e:KeyboardEvent)=>{
     const k=parseKey(e);
     if(k&&shortcutRecording){
       if(e.key==="Escape"){setShortcutRecording(null);return}
       if(!/Control|Alt/.test(k)){setError("请使用带 Ctrl 或 Alt 的组合键");return}
       if(shortcutRecording==="global"){
         e.preventDefault();
         const hk=k;
         setGlobalHotkey(hk);localStorage.setItem("lv_global_hotkey",hk);
         setShortcutRecording(null);setError("");
         if(globalHotkeyEnabled){
           const win=getCurrentWindow();
           void gsUnregister(globalHotkey).catch(()=>{});
           void gsRegister(hk,()=>{void win.show();void win.setFocus()}).then(()=>setGlobalHotkeyErr("")).catch((er)=>setGlobalHotkeyErr("全局快捷键注册失败："+String(er)));
         }
         return
       }
       const dup=Object.entries(shortcuts).find(([kk,vv])=>vv===k&&kk!==shortcutRecording);
       if(dup){setError(`与「${shortcutLabel(dup[0] as "search"|"save"|"lock")}」冲突`);return}
       e.preventDefault();setShortcuts(s=>({...s,[shortcutRecording]:k}));setShortcutRecording(null);setError("");return
     }
     if(k&&k===shortcuts.search){if(view==="vault"){e.preventDefault();searchRef.current?.focus();return}}
     if(k&&k===shortcuts.save){if(view==="vault"&&editMode&&draft){e.preventDefault();void commitDraft();return}}
     if(k&&k===shortcuts.lock){if(view==="vault"){e.preventDefault();void lock();return}}
     if(e.key==="Escape"){
       if(closeChoiceOpen){setCloseChoiceOpen(false);return}
       if(menu){setMenu(null);return}
       if(dialog!=="none"){setDialog("none");setPendingFile(null);setCredentialPassword("");setBackupData(null);setVerifiedMasterPassword("");setError("")}
     }
   };
   addEventListener("keydown",onKey);
   return()=>removeEventListener("keydown",onKey);
 },[view,menu,dialog,shortcuts,shortcutRecording,editMode,draft,closeChoiceOpen,globalHotkey,globalHotkeyEnabled]);
 // 点击窗口 ×：默认不直接退出——按"记住的选择"或弹出选择窗口（退出 / 最小化到托盘）
 useEffect(()=>{
   let unlisten:(()=>void)|undefined;
   const setup=async()=>{
     try{
       const win=getCurrentWindow();
       unlisten=await win.onCloseRequested(async (event)=>{
         // 仅已解锁的主界面启用"最小化/退出"选择；解锁、创建主密码等界面点 × 直接关闭
         if(viewRef.current!=="vault"){return}
         let pref:ClosePref;
         try{const raw=localStorage.getItem("lv_close_pref");pref=raw?JSON.parse(raw):{action:"tray",remember:false}}catch{pref={action:"tray",remember:false}}
         if(pref.remember&&pref.action==="tray"){
           event.preventDefault();
           try{await win.hide()}catch{}
           return;
         }
         if(pref.remember&&pref.action==="quit"){
           return;
         }
         event.preventDefault();
         setCloseChoiceAction("tray");
         setCloseRemember(false);
         setCloseChoiceOpen(true);
       });
     }catch{}
   };
   setup();
   return()=>{if(unlisten)unlisten()};
 },[]);
 // 全局唤出快捷键：启动时按保存的配置注册（默认 Ctrl+Alt+L，默认关闭）
 useEffect(()=>{
   let hk=DEFAULT_GLOBAL_HOTKEY;
   const setup=async()=>{
     try{
       try{const raw=localStorage.getItem("lv_global_hotkey");if(raw)hk=raw}catch{}
       let enabled=false;
       try{const rawE=localStorage.getItem("lv_global_hotkey_enabled");enabled=rawE?rawE==="1":false}catch{}
       setGlobalHotkey(hk);setGlobalHotkeyEnabled(enabled);
       if(!enabled)return;
       const win=getCurrentWindow();
       try{await gsRegister(hk,()=>{void win.show();void win.setFocus()});setGlobalHotkeyErr("")}catch(e){setGlobalHotkeyErr("全局快捷键注册失败："+String(e))}
     }catch{}
   };
   setup();
   return()=>{void gsUnregister(hk).catch(()=>{})};
 },[]);
 useLayoutEffect(()=>{
   if(!menu||!menuRef.current)return;
   const el=menuRef.current;
   const pad=8;
   let x=menu.x,y=menu.y;
   if(x+el.offsetWidth>window.innerWidth-pad)x=Math.max(pad,window.innerWidth-el.offsetWidth-pad);
   if(y+el.offsetHeight>window.innerHeight-pad)y=Math.max(pad,window.innerHeight-el.offsetHeight-pad);
   el.style.left=`${x}px`;el.style.top=`${y}px`;
 },[menu]);
 useEffect(()=>{
   if(view==="vault"&&!localStorage.getItem("lv_onboarding_seen"))setShowOnboarding(true);
 },[view]);

 useEffect(()=>{
   const onMove=(ev:PointerEvent)=>{
     const d=pointerDrag.current;if(!d)return;
     const distance=Math.hypot(ev.clientX-d.startX,ev.clientY-d.startY);
     if(!d.active){if(distance<6)return;d.active=true;d.didDrag=true;suppressClick.current=true;dragChangedRef.current=false;if(d.kind==="entry"){setDraggedId(d.id);dragOverIdRef.current=d.id;setDragOverId(d.id)}else{setDraggedCategory(d.id);dragOverCategoryRef.current=d.id;setDragOverCategory(d.id)}ev.preventDefault();}
     ev.preventDefault();setDragPoint({x:ev.clientX,y:ev.clientY});
     if(d.kind==="entry"){
       if(query.trim()||sortMode!=="manual")return;
       const candidates=Array.from(document.querySelectorAll<HTMLElement>('.item[data-entry-id]')).filter(el=>el.dataset.entryId!==d.id);
       let targetId:string|null=null;let before=true;
       for(const el of candidates){const r=el.getBoundingClientRect();if(ev.clientY>=r.top&&ev.clientY<=r.bottom){targetId=el.dataset.entryId||null;before=ev.clientY<r.top+r.height/2;break}}
       if(!targetId&&candidates.length){const last=candidates[candidates.length-1];const r=last.getBoundingClientRect();if(ev.clientY>r.bottom){targetId=last.dataset.entryId||null;before=false}else{const first=candidates[0];const fr=first.getBoundingClientRect();if(ev.clientY<fr.top){targetId=first.dataset.entryId||null;before=true}}}
       if(targetId){
         const current=entriesRef.current.filter(entryVisible);
         const from=current.findIndex(e=>e.id===d.id);const target=current.findIndex(e=>e.id===targetId);
         if(from>=0&&target>=0){
           let insert=target+(before?0:1);
           if(from<insert)insert--;
           insert=Math.max(0,Math.min(insert,current.length-1));
           if(insert!==from){
             const reordered=[...current];const [m]=reordered.splice(from,1);reordered.splice(insert,0,m);
             const visibleIds=new Set(reordered.map(e=>e.id));let cursor=0;
             const xs=entriesRef.current.map(e=>visibleIds.has(e.id)?reordered[cursor++]:e);
             const normalized=renumber(xs);
             entriesRef.current=normalized;setEntries(normalized);dragChangedRef.current=true;
             dragOverIdRef.current=targetId;setDragOverId(targetId);
           }
         }
       }
     }else{
       const candidates=Array.from(document.querySelectorAll<HTMLElement>('.categoryItem[data-category-name]')).filter(el=>el.dataset.categoryName!==d.id);
       let targetId:string|null=null;let before=true;
       for(const el of candidates){const r=el.getBoundingClientRect();if(ev.clientY>=r.top&&ev.clientY<=r.bottom){targetId=el.dataset.categoryName||null;before=ev.clientY<r.top+r.height/2;break}}
       if(targetId){const current=categoriesRef.current;const source=current.find(c=>c.name===d.id);const target=current.find(c=>c.name===targetId);if(source&&target&&source.parentName===target.parentName){const siblings=current.filter(c=>c.parentName===source.parentName);const from=siblings.findIndex(c=>c.name===d.id);const ti=siblings.findIndex(c=>c.name===targetId);let insert=ti+(before?0:1);if(from<insert)insert--;if(insert!==from){const reordered=[...siblings];const [m]=reordered.splice(from,1);reordered.splice(Math.max(0,Math.min(insert,reordered.length)),0,m);const positions=siblings.map(x=>current.findIndex(c=>c.name===x.name));const next=[...current];positions.forEach((pos,i)=>{next[pos]=reordered[i]});categoriesRef.current=next;setCategories(next);dragChangedRef.current=true;dragOverCategoryRef.current=targetId;setDragOverCategory(targetId)}}}
     }
   };
   const onUp=()=>{
     const d=pointerDrag.current;pointerDrag.current=null;if(!d)return;
     const changed=dragChangedRef.current;dragChangedRef.current=false;setDraggedId(null);setDraggedCategory(null);setDragPoint(null);dragOverIdRef.current=null;dragOverCategoryRef.current=null;setDragOverId(null);setDragOverCategory(null);
     if(!d.active){return}
     if(d.kind==="entry"){if(changed){const finalEntries=entriesRef.current.map(e=>({...e}));void saveEntries(finalEntries)}else{suppressClick.current=false}}
     else if(changed){const source=categoriesRef.current.find(c=>c.name===d.id);if(source){const names=categoriesRef.current.filter(c=>c.parentName===source.parentName).map(c=>c.name);void invoke("category_reorder",{parentName:source.parentName,names}).catch(e=>setError(String(e)))}}
     suppressClick.current=true;setTimeout(()=>{suppressClick.current=false},0);
   };
   addEventListener("pointermove",onMove,{passive:false});addEventListener("pointerup",onUp);addEventListener("pointercancel",onUp);
   return()=>{removeEventListener("pointermove",onMove);removeEventListener("pointerup",onUp);removeEventListener("pointercancel",onUp)};
 },[query,category,securityFilter,categories,sortMode]);

 const loadCategories=async()=>{try{const cs=await invoke<Category[]>("category_list");setCategories(cs.length?cs:DEFAULT_CATS)}catch{setCategories(DEFAULT_CATS)}};
 const loadBackupSettings=async()=>{try{const bs=await invoke<BackupSettings>("backup_settings_get");setBackupSettings(bs);setBackupEnabled(bs.enabled);setBackupDirectory(bs.directory||"");setBackupRetention(bs.retention||5);return bs}catch{return backupSettings}};
 const loadHistory=async(id:string)=>{try{setHistory(await invoke<History[]>("history_list",{entryId:id}))}catch{setHistory([])}};
 const loadRecoveryQuestions=async()=>{try{const qs=await invoke<string[]>("recovery_questions");if(qs.length===3)setQuestions(qs)}catch{}};
 const copyRecoveryCode=async(code:string)=>{try{await navigator.clipboard.writeText(code);setSaveState("Recovery Code 已复制");return true}catch{setError("无法自动复制 Recovery Code，请从提示框中手动保存");return false}};
 const openRecoveryUnlock=async()=>{setError("");await loadRecoveryQuestions();setRecoveryCode("");setAnswers(["","",""]);setView("recoveryUnlock")};
 const createVault=async()=>{setError("");if(!validMasterPassword(master)){setError("主密码必须至少 8 位，并包含数字、大写字母、小写字母和特殊符号");return}if(master!==confirmMaster){setError("两次输入的主密码不一致");return}try{await invoke("vault_create",{masterPassword:master,confirmPassword:confirmMaster});const code=await invoke<string>("recovery_generate_code");setGeneratedRecoveryCode(code);setMaster("");setConfirmMaster("");await loadCategories();setView("recoverySetup")}catch(e){setError(String(e))}};
 const finishRecovery=async()=>{setError("");try{await invoke("recovery_enable",{recoveryCode:generatedRecoveryCode,recoveryQuestions:questions,recoveryAnswers:answers});await copyRecoveryCode(generatedRecoveryCode);setGeneratedRecoveryCode("");setAnswers(["","",""]);setView("vault");lastActivityRef.current=now()}catch(e){setError(String(e))}};
 const unlock=async()=>{
   setError("");
   const remaining=unlockSecurity.lockUntil?Math.max(0,unlockSecurity.lockUntil-Date.now()):0;
   if(remaining>0){setError(`请稍候再试，当前已锁定 ${Math.max(1,Math.ceil(remaining/1000))} 秒`);return}
   try{
     const xs=await invoke<Entry[]>("vault_unlock",{masterPassword:master,captcha:unlockSecurity.captchaRequired?captchaInput:null});
     const normalized=normalizeEntries(xs.map((e,i)=>({...e,seq:e.seq||i+1,type:e.type||"网站",email:e.email||"",phone:e.phone||"",nickname:e.nickname||"",expiresAt:e.expiresAt??null})));
     setEntries(normalized);await loadCategories();await loadBackupSettings();await loadTrash();setSelected(null);setMaster("");setCaptchaInput("");setUnlockSecurity({failedAttempts:0,captchaRequired:false,lockUntil:null,captchaSvg:null});setView("vault");lastActivityRef.current=now();void loadAutofillStatus()
   }catch(e){
     const msg=String(e);
     // Always re-sync security state after an unlock failure. The backend may have
     // recorded a password failure and rotated the CAPTCHA, or rotated it because
     // the submitted CAPTCHA itself was invalid. Never keep a stale challenge.
     await refreshUnlockSecurity();
     setCaptchaInput("");
     setError(msg.includes("验证码")||msg.includes("锁定")?msg:"主密码错误，或 Vault 已损坏/篡改");
   }
 };
 const refreshCaptcha=async()=>{try{const svg=await invoke<string>("vault_refresh_captcha");setUnlockSecurity(s=>({...s,captchaSvg:svg}));setCaptchaInput("")}catch(e){setError(String(e))}};
 const verifyRecovery=async()=>{setError("");try{await invoke("recovery_verify",{recoveryCode,answers});setRecoveryCode("");setAnswers(["","",""]);setNewMaster("");setNewConfirm("");setView("recoveryReset")}catch{setError("恢复验证失败：Recovery Code 或密保答案不正确")}};
 const resetMasterFromRecovery=async()=>{setError("");if(!validMasterPassword(newMaster)){setError("新主密码必须至少 8 位，并包含数字、大写字母、小写字母和特殊符号");return}if(newMaster!==newConfirm){setError("两次输入的新主密码不一致");return}try{await invoke("recovery_set_master",{newPassword:newMaster,confirmPassword:newConfirm});setNewMaster("");setNewConfirm("");setView("unlock");setMaster("");setError("主密码已修改，请使用新主密码验证解锁。")}catch(e){setError(String(e))}};
 const cancelRecoveryReset=async()=>{await invoke("recovery_cancel").catch(()=>{});setNewMaster("");setNewConfirm("");setView("unlock")};
 const openSecuritySettings=async()=>{if(dirty){askDiscard(openSecuritySettings);return}setError("");setCredentialKind("settings");setCredentialPassword("");setDialog("credential")};
 const askDiscard=(action:()=>void|Promise<void>)=>{setPendingAction(()=>action);setDialog("discard")};
 const performLock=async()=>{await invoke("vault_lock").catch(()=>{});clearRevealed();setEntries([]);setSelected(null);setDraft(null);setEditMode(false);setDirty(false);setView("unlock");setMaster("");void loadAutofillStatus()};
 const performAdd=()=>{const c=category==="全部"?"默认":category;const e=emptyEntry(1,c);setSelected(e.id);setDraft(e);setEditMode(true);setDirty(false);setShowPass(false);setMenu(null)};
 const performChoose=(id:string)=>{const e=entries.find(x=>x.id===id);if(!e)return;clearRevealed();setSelected(id);setDraft({...e});setEditMode(false);setShowPass(false);setDirty(false);loadHistory(id)};
 const requestAction=(action:()=>void|Promise<void>)=>{if(dirty)askDiscard(action);else void action()};
 const lock=()=>requestAction(performLock);
 const loadAutofillStatus=async()=>{try{const s=await invoke<AutofillStatus>("autofill_status");setAutofill(s)}catch{}};
 const openAutofillDialog=async()=>{setError("");await loadAutofillStatus();setPairCode("");setDialog("autofill")};
 const openGeneralSettings=async()=>{setError("");try{setAutostart(await autostartIsEnabled())}catch(e){setError(String(e))}try{const raw=localStorage.getItem("lv_close_pref");const p=raw?JSON.parse(raw):{action:"tray",remember:false};setClosePref(p)}catch{}setDialog("generalSettings")};
 const toggleAutostart=async(on:boolean)=>{setError("");try{if(on){await autostartEnable()}else{await autostartDisable()}setAutostart(on);setSaveState(on?"已开启开机自启":"已关闭开机自启")}catch(e){setError("开机自启设置失败："+String(e));try{setAutostart(await autostartIsEnabled())}catch{}}};
 const applyCloseChoice=async()=>{const action=closeChoiceAction;const remember=closeRemember;const win=getCurrentWindow();setCloseChoiceOpen(false);if(remember){localStorage.setItem("lv_close_pref",JSON.stringify({action,remember:true}));setClosePref({action,remember:true})}if(action==="quit"){try{await win.destroy()}catch{}}else{try{await win.hide()}catch{}}};
 const resetClosePref=()=>{localStorage.removeItem("lv_close_pref");setClosePref({action:"tray",remember:false});setSaveState("已恢复为每次询问关闭方式")};
 const toggleGlobalHotkey=async(on:boolean)=>{setGlobalHotkeyErr("");try{if(on){const win=getCurrentWindow();await gsRegister(globalHotkey,()=>{void win.show();void win.setFocus()})}else{await gsUnregister(globalHotkey).catch(()=>{})}setGlobalHotkeyEnabled(on);localStorage.setItem("lv_global_hotkey_enabled",on?"1":"0")}catch(e){setGlobalHotkeyErr("全局快捷键"+(on?"开启":"关闭")+"失败："+String(e))}};
 const toggleAutofill=async(on:boolean)=>{try{const s=await invoke<AutofillStatus>("autofill_toggle",{enabled:on});setAutofill(s);setPairCode("")}catch(e){setError(String(e));void loadAutofillStatus()}};
 const genPairCode=async()=>{setError("");try{const c=await invoke<string>("autofill_begin_pair");setPairCode(c)}catch(e){setError(String(e))}};
 const unpairAll=async()=>{setError("");try{await invoke("autofill_unpair_all");await loadAutofillStatus()}catch(e){setError(String(e))}};
 const saveEntries=async(xs:Entry[]):Promise<Entry[]|null>=>{setSaveState("保存中…");try{const saved=await invoke<Entry[]>("vault_save",{entries:xs});setEntries(saved);const bs=await loadBackupSettings();setSaveState(bs.enabled&&bs.lastError?"已保存（自动备份失败）":"已保存");setDirty(false);return saved}catch(e){setSaveState("保存失败");const msg=String(e);if(/vault locked/i.test(msg)){clearRevealed();setEntries([]);setSelected(null);setDraft(null);setEditMode(false);setDirty(false);setView("unlock");setMaster("");void loadAutofillStatus();}setError(msg);return null}};
 const clearRevealed=()=>{if(revealTimerRef.current){clearTimeout(revealTimerRef.current);revealTimerRef.current=null}setRevealed(null)};
 const revealPassword=async(e:Entry)=>{clearRevealed();if(!e.passwordEncrypted){setError("该条目未设置密码");return}try{const text=await invoke<string>("vault_entry_password",{entryId:e.id,passwordEncrypted:e.passwordEncrypted});setRevealed({id:e.id,text});revealTimerRef.current=window.setTimeout(()=>{setRevealed(null);revealTimerRef.current=null},20000)}catch(err){setError(String(err))}};
 const renumber=(xs:Entry[])=>xs.map((e,i)=>({...e,seq:i+1}));
 const normalizeEntries=(xs:Entry[])=>{const valid=xs.length===0||xs.every(e=>Number.isFinite(e.seq)&&e.seq>0);if(!valid)return renumber(xs);const sorted=[...xs].sort((a,b)=>a.seq-b.seq);return renumber(sorted)};
 const descendantNames=(name:string)=>{const out=[name];let changed=true;while(changed){changed=false;for(const c of categories){if(c.parentName&&out.includes(c.parentName)&&!out.includes(c.name)){out.push(c.name);changed=true}}}return out};
 const categoryMatches=(entry:Entry)=>category==="全部"||category==="收藏"&&entry.favorite||category!=="收藏"&&descendantNames(category).includes(entry.category);
 const entryVisible=(e:Entry)=>categoryMatches(e)&&securityMatches(e)&&(!query.trim()||(`${e.name} ${e.username} ${e.email} ${e.phone} ${e.nickname} ${e.url} ${e.category} ${e.tags.join(" ")} ${e.type}`).toLowerCase().includes(query.toLowerCase()));
 const visibleEntries=()=>entries.filter(entryVisible);
 const reorderVisible=async(fromId:string,toId:string)=>{
   if(query.trim()){setError("搜索结果下不能调整排序，请先清空搜索");return false}
   const visible=visibleEntries(),from=visible.findIndex(e=>e.id===fromId),to=visible.findIndex(e=>e.id===toId);
   if(from<0||to<0||from===to)return false;
   const reordered=[...visible];const [m]=reordered.splice(from,1);reordered.splice(to,0,m);
   const visibleIds=new Set(reordered.map(e=>e.id));let cursor=0;
   const xs=entries.map(e=>visibleIds.has(e.id)?reordered[cursor++]:e);
   const normalized=renumber(xs);
   const saved=await saveEntries(normalized);
   if(saved){setSelected(fromId);setDraft(d=>d&&d.id===fromId?{...d,seq:normalized.find(e=>e.id===fromId)?.seq||d.seq}:d)}
   return saved;
 };
 const moveEntry=async(id:string,delta:-1|1|"top")=>{
   if(query.trim()){setError("搜索结果下不能调整排序，请先清空搜索");return}
   if(sortMode!=="manual"){setError("当前为自动排序，请先切换回手动排序再调整顺序");return}
   const visible=visibleEntries(),from=visible.findIndex(e=>e.id===id);if(from<0)return;let to=delta==="top"?0:from+delta;if(to<0)to=0;if(to>=visible.length)to=visible.length-1;if(to===from)return;void reorderVisible(id,visible[to].id);
 };
 const reorderCategories=async(fromName:string,toName:string)=>{const source=categories.find(c=>c.name===fromName),target=categories.find(c=>c.name===toName);if(!source||!target||source.parentName!==target.parentName)return false;const siblings=categories.filter(c=>c.parentName===source.parentName);const from=siblings.findIndex(c=>c.name===fromName),to=siblings.findIndex(c=>c.name===toName);if(from<0||to<0||from===to)return false;const reordered=[...siblings];const [m]=reordered.splice(from,1);reordered.splice(to,0,m);const xs=[...categories];siblings.forEach((c,i)=>{xs[xs.findIndex(x=>x.name===c.name)]=reordered[i]});try{await invoke("category_reorder",{parentName:source.parentName,names:reordered.map(c=>c.name)});setCategories(xs);return true}catch(e){setError(String(e));return false}};
 const moveCategory=async(name:string,delta:-1|1|"top")=>{const source=categories.find(c=>c.name===name);if(!source)return;const siblings=categories.filter(c=>c.parentName===source.parentName);const from=siblings.findIndex(c=>c.name===name);if(from<0)return;let to=delta==="top"?0:from+delta;if(to<0)to=0;if(to>=siblings.length)to=siblings.length-1;if(to===from)return;await reorderCategories(name,siblings[to].name)};
 const toggleCategory=(name:string)=>{setCollapsedCats(a=>a.includes(name)?a.filter(x=>x!==name):[...a,name])};
 const toggleFavorite=async(id:string)=>{const xs=entries.map(e=>e.id===id?{...e,favorite:!e.favorite,updatedAt:now()}:e);await saveEntries(xs);if(draft?.id===id)setDraft({...draft,favorite:!draft.favorite,updatedAt:now()});};
 const add=()=>requestAction(performAdd);
 const choose=(id:string)=>{if(dirty&&selected!==id){askDiscard(()=>performChoose(id));return}performChoose(id)};
 const commitDraft=async()=>{if(!draft)return true;const isNew=!entries.some(e=>e.id===draft.id);const hasContent=[draft.name!=="新密码"?draft.name:"",draft.username,draft.email,draft.phone,draft.password,draft.nickname,draft.url,draft.notes,draft.tags.join("")].some(v=>String(v||"").trim());if(isNew&&!hasContent){clearRevealed();setSelected(null);setDraft(null);setEditMode(false);setDirty(false);return true}const updated={...draft,updatedAt:now(),name:draft.name.trim()||"未命名"};const xs=isNew?renumber([updated,...entries]):entries.map(e=>e.id===draft.id?updated:e);const saved=await saveEntries(xs);if(!saved)return false;const savedEntry=saved.find(e=>e.id===updated.id)||updated;clearRevealed();setDraft(savedEntry);setSelected(updated.id);setEditMode(false);await loadHistory(updated.id);return true};
 const deleteCurrent=async()=>{if(!selected)return;const exists=entries.some(e=>e.id===selected);if(!exists){clearRevealed();setSelected(null);setDraft(null);setEditMode(false);setDialog("none");return}try{await invoke("trash_move",{entryId:selected});setEntries(renumber(entries.filter(e=>e.id!==selected)));void loadTrash();}catch(e){setError(String(e));return}clearRevealed();setSelected(null);setDraft(null);setEditMode(false);setDirty(false);setDialog("none")};
 const loadTrash=async()=>{try{setTrash(await invoke<Entry[]>("trash_list",{retentionDays:trashDays}))}catch{setTrash([])}};
 const restoreTrash=async(id:string)=>{try{const xs=await invoke<Entry[]>("trash_restore",{entryId:id});setEntries(normalizeEntries(xs));await loadTrash();setDialog("trash")}catch(e){setError(String(e))}};
 const purgeTrash=async(id?:string)=>{try{await invoke("trash_purge",{entryId:id||null});await loadTrash()}catch(e){setError(String(e))}};
 const updateCategory=async()=>{if(!categoryEdit)return;const n=categoryName.trim();if(!n){setError("请输入分类名称");return}try{await invoke("category_update",{oldName:categoryEdit.name,name:n,icon:categoryIcon,parentName:categoryParent});const xs=entries.map(e=>e.category===categoryEdit.name?{...e,category:n}:e);await saveEntries(xs);if(category===categoryEdit.name)setCategory(n);await loadCategories();setCategoryEdit(null);setDialog("none")}catch(e){setError(String(e))}};
 const deleteCategory=async(name:string)=>{if(name==="默认"){setError("默认分类不能删除");return}try{const xs=entries.map(e=>e.category===name?{...e,category:"默认"}:e);await invoke("category_delete",{name});await saveEntries(xs);if(category===name)setCategory("全部");await loadCategories();}catch(e){setError(String(e))}};
 const openTypePrompt=()=>{setPromptValue("");setPromptMeta({kind:"type",title:"自定义账号类型",placeholder:"输入新的账号类型"});setDialog("prompt")};
 const openTagPrompt=()=>{setPromptValue("");setPromptMeta({kind:"tag",title:"自定义标签",placeholder:"输入新的标签"});setDialog("prompt")};
 const confirmPrompt=()=>{const v=promptValue.trim();if(!v)return;if(promptMeta?.kind==="type"){setTypes(a=>{const x=Array.from(new Set([...a,v]));localStorage.setItem("lv_types",JSON.stringify(x));return x})}else if(promptMeta?.kind==="tag"){setTags(a=>{const x=Array.from(new Set([...a,v]));localStorage.setItem("lv_tags",JSON.stringify(x));return x})}else if(promptMeta?.kind==="batchtag"){const tagList=v.split(/[、,，]/).map(x=>x.trim()).filter(Boolean);if(!tagList.length){setPromptMeta(null);setPromptValue("");setDialog("none");return}const ids=new Set(multiSelected);const xs=entries.map(e=>ids.has(e.id)?{...e,tags:Array.from(new Set([...e.tags,...tagList])),updatedAt:now()}:e);void saveEntries(xs).then(ok=>{if(ok)setMultiSelected([])});setSaveState(`已为 ${ids.size} 条添加标签`)}setPromptMeta(null);setPromptValue("");setDialog("none")};
 const downloadBulkTemplate=async()=>{try{const p=await save({defaultPath:"LocalVault-批量导入模板.csv",filters:[{name:"CSV 模板",extensions:["csv"]}]});if(!p)return;await invoke("bulk_import_template",{destination:p});setSaveState("批量导入模板已生成")}catch(e){setError(String(e))}};
 const importBulkTemplate=async()=>{try{const p=await open({multiple:false,directory:false,filters:[{name:"LocalVault 批量导入模板",extensions:["csv"]}]});if(typeof p!=="string")return;setError("");const text=await invoke<string>("bulk_import_read",{source:p});const parsed=parseBulkEntries(text);if(!parsed.entries.length){setBulkDemoStatus(parsed.demoRowsFound?`已识别并忽略 ${parsed.demoRowsFound} 条模板示例数据；没有检测到可导入的实际数据。`:"未检测到模板示例数据，说明示例行已删除；当前也没有可导入的实际数据。");setError("模板中没有可导入的有效数据行");return}setImported(parsed.entries);setBulkDemoStatus(parsed.demoRowsFound?`已识别并自动忽略 ${parsed.demoRowsFound} 条模板示例数据，剩余 ${parsed.entries.length} 条实际数据待导入。`:`未检测到模板示例数据，说明示例行已删除；检测到 ${parsed.entries.length} 条实际数据。`);const existing=new Map(entries.map(e=>[`${e.type}::${e.username}`.toLowerCase(),e]));const choices:Record<string,"replace"|"skip">={};for(const e of parsed.entries){const key=`${e.type}::${e.username}`.toLowerCase();if(existing.has(key)&&e.username)choices[e.id]="skip"}setConflictChoice(choices);setDialog("import")}catch(e){setError(String(e))}};
 const openBulkImport=()=>{setError("");setBulkDemoStatus("");setDialog("bulkImport")};
 const exportEntries=async()=>{try{const p=await save({defaultPath:"LocalVault-仅账号密码.lvx",filters:[{name:"LocalVault 账号密码数据",extensions:["lvx"]}]});if(p){setPendingFile(p);setCredentialKind("export");setCredentialPassword("");setDialog("credential")}}catch(e){setError(String(e))}};
 const importEntries=async()=>{try{const p=await open({multiple:false,directory:false,filters:[{name:"LocalVault 账号密码数据",extensions:["lvx"]}]});if(typeof p!=="string")return;setPendingFile(p);setCredentialKind("import");setCredentialPassword("");setDialog("credential")}catch(e){setError(String(e))}};
 const restore=async()=>{try{const p=await open({multiple:false,directory:false,filters:[{name:"LocalVault 加密备份",extensions:["vault"]}]});if(typeof p!=="string")return;setPendingFile(p);setCredentialKind("restore");setCredentialPassword("");setDialog("credential")}catch(e){setError(String(e))}};
 const verifyBackupFile=async()=>{try{const p=await open({multiple:false,directory:false,filters:[{name:"LocalVault 加密备份",extensions:["vault"]}]});if(typeof p!=="string")return;setPendingFile(p);setCredentialKind("verify");setCredentialPassword("");setDialog("credential")}catch(e){setError(String(e))}};
 const confirmCredential=async()=>{const p=pendingFile;const pw=credentialPassword;setError("");try{if(credentialKind==="settings"){await invoke("vault_verify_master",{masterPassword:pw});try{const qs=await invoke<string[]>("recovery_questions");if(qs.length===3)setSecurityQuestions(qs)}catch{}setSecurityAnswers(["","",""]);setChangeMaster(false);setChangeRecovery(false);setNewMaster("");setNewConfirm("");setVerifiedMasterPassword(pw);setCredentialPassword("");setDialog("securitySettings");return}if(!p){setDialog("none");return}if(credentialKind==="backup"){await invoke("vault_verify_master",{masterPassword:pw});await invoke("vault_backup",{destination:p});setSaveState("已导出完整备份");setPendingFile(null);setCredentialPassword("");setDialog("none")}else if(credentialKind==="export"){await invoke("vault_export",{entryIds:entries.map(e=>e.id),destination:p,masterPassword:pw});setSaveState("已导出仅账号密码");setDialog("none")}else if(credentialKind==="verify"){const data=await invoke<BackupData>("vault_backup_verify",{backupPath:p,masterPassword:pw});setBackupData(data);setSaveState(`备份校验通过：${data.entries.length} 条密码`);setPendingFile(null);setCredentialPassword("");setDialog("none")}else if(credentialKind==="import"){const xs=await invoke<Entry[]>("vault_import",{source:p,sourceMasterPassword:pw});const existing=new Map(entries.map(e=>[`${e.type}::${e.username}`.toLowerCase(),e]));const choices:Record<string,"replace"|"skip">={};for(const e of xs){const key=`${e.type}::${e.username}`.toLowerCase();if(existing.has(key)&&e.username)choices[e.id]="skip";}setImported(xs);setConflictChoice(choices);setPendingFile(null);setCredentialPassword("");setDialog("import")}else{const data=await invoke<BackupData>("vault_backup_preview",{backupPath:p,masterPassword:pw});setBackupData(data);if(view==="vault"){setDialog("backupRestore")}else{const xs=await invoke<Entry[]>("vault_restore",{backupPath:p,masterPassword:pw});clearRevealed();setEntries(normalizeEntries(xs));await loadCategories();await loadTrash();setSelected(null);setDraft(null);setEditMode(false);setDirty(false);setMaster("");setPendingFile(null);setCredentialPassword("");setBackupData(null);setDialog("none");setView("vault");lastActivityRef.current=now()}}}catch(e){setError(String(e))}};
 const saveSecuritySettings=async()=>{if(changeMaster&&!validMasterPassword(newMaster)){setError("新主密码必须至少 8 位，并包含数字、大写字母、小写字母和特殊符号");return}if(changeMaster&&newMaster===verifiedMasterPassword){setError("新主密码不能与旧主密码相同");return}if(!changeMaster&&!changeRecovery){setError("请至少选择一项修改");return}if(changeMaster&&newMaster!==newConfirm){setError("两次输入的新主密码不一致");return}if(changeRecovery&&securityAnswers.some(a=>!a.trim())){setError("每个密保答案不能为空");return}setError("");try{const code=await invoke<string|null>("vault_update_security",{currentPassword:verifiedMasterPassword,newPassword:changeMaster?newMaster:null,newConfirm:changeMaster?newConfirm:null,questions:changeRecovery?securityQuestions:null,answers:changeRecovery?securityAnswers:null});if(code){await copyRecoveryCode(code);setNewRecoveryCode(code);setCredentialPassword("");setVerifiedMasterPassword("");setNewMaster("");setNewConfirm("");setChangeMaster(false);setChangeRecovery(false);setSaveState("安全设置已更新");setDialog("recoveryCode")}else{setCredentialPassword("");setVerifiedMasterPassword("");setNewMaster("");setNewConfirm("");setChangeMaster(false);setChangeRecovery(false);setDialog("none");setSaveState("安全设置已更新")}}catch(e){setError(String(e))}};
 const restoreBackupReplace=async()=>{const p=pendingFile;const pw=credentialPassword;if(!p)return;setError("");try{const xs=await invoke<Entry[]>("vault_restore",{backupPath:p,masterPassword:pw});setEntries(normalizeEntries(xs));await loadCategories();await loadTrash();setSelected(null);setDraft(null);setEditMode(false);setDirty(false);setMaster("");setPendingFile(null);setCredentialPassword("");setBackupData(null);setDialog("none");setView("vault");lastActivityRef.current=now()}catch(e){setError(String(e))}};
 const restoreBackupMerge=async()=>{const p=pendingFile;const pw=credentialPassword;if(!p||!backupData)return;setError("");try{const xs=await invoke<Entry[]>("vault_backup_merge",{backupPath:p,masterPassword:pw});setEntries(normalizeEntries(xs));await loadCategories();await loadTrash();setSelected(null);setDraft(null);setEditMode(false);setDirty(false);setMaster("");setPendingFile(null);setCredentialPassword("");setBackupData(null);setDialog("none");setSaveState("已合并 Vault 备份")}catch(e){setError(String(e))}};
 const batchDelete=async()=>{if(!multiSelected.length)return;setDialog("delete");};
 const batchFavorite=async(fav:boolean)=>{if(!multiSelected.length)return;const ids=new Set(multiSelected);const xs=entries.map(e=>ids.has(e.id)?{...e,favorite:fav,updatedAt:now()}:e);const ok=await saveEntries(xs);if(ok){setMultiSelected([]);setSaveState(fav?`已收藏 ${ids.size} 条`:`已取消收藏 ${ids.size} 条`)}};
 const batchAddTag=()=>{if(!multiSelected.length)return;setPromptValue("");setPromptMeta({kind:"batchtag",title:`批量添加标签（已选 ${multiSelected.length} 条）`,placeholder:"输入要添加的标签，多个用、分隔"});setDialog("prompt")};
 const listCopyPassword=async(e:Entry)=>{if(!e.passwordEncrypted){setError("该条目未设置密码");return}try{await invoke("vault_copy_password",{entryId:e.id,passwordEncrypted:e.passwordEncrypted});setListCopied(e.id);setTimeout(()=>setListCopied(c=>c===e.id?null:c),1500)}catch(err){setError(String(err))}};
 const copyEntryInfo=async()=>{const e=entries.find(x=>x.id===menu?.id);if(!e){setMenu(null);return}try{const ne=await invoke<Entry>("vault_duplicate_entry",{sourceId:e.id});const xs=renumber([ne,...entries]);setEntries(xs);setSelected(ne.id);setDraft(ne);setEditMode(true);setDirty(true);setMenu(null);setSaveState("已复制为副本，请修改后保存")}catch(err){setError(String(err));setMenu(null)}};
 const copyEntryField=async(kind:"username"|"password"|"url")=>{const e=entries.find(x=>x.id===menu?.id);if(!e){setMenu(null);return}const label=kind==="username"?"账号":kind==="password"?"密码":"网址";if(kind==="password"){if(!e.passwordEncrypted){setError("该条目未设置密码");setMenu(null);return}try{await invoke("vault_copy_password",{entryId:e.id,passwordEncrypted:e.passwordEncrypted});setSaveState("密码已复制")}catch(err){setError(String(err))}setMenu(null);return}const v=e[kind];if(!v){setError(`该条目未填写${label}`);setMenu(null);return}const ok=await copyText(v,false);if(ok){setSaveState(`${label}已复制`)}setMenu(null)};
 const applyBatchDelete=async()=>{const ids=[...multiSelected];const okIds:string[]=[];let lastErr="";for(const id of ids){try{await invoke("trash_move",{entryId:id});okIds.push(id)}catch(e){lastErr=String(e);break}}if(okIds.length){setEntries(renumber(entries.filter(e=>!okIds.includes(e.id))));if(draft&&okIds.includes(draft.id)){setDraft(null);setSelected(null)}}const failed=multiSelected.filter(id=>!okIds.includes(id));if(failed.length){setMultiSelected(failed);setError(lastErr||`部分删除失败：${okIds.length}/${ids.length} 条已移入回收站`);return}clearRevealed();setMultiSelected([]);setSelected(null);setDraft(null);setEditMode(false);setDirty(false);setDialog("none");void loadTrash()};
 const applyImport=async()=>{const existing=new Map(entries.map(e=>[`${e.type}::${e.username}`.toLowerCase(),e]));let xs=[...entries];const missingCats=Array.from(new Set(imported.map(e=>e.category||"默认").filter(c=>!categories.some(x=>x.name===c))));try{for(const c of missingCats){await invoke("category_create",{name:c,icon:"📁",parentName:null})}if(missingCats.length)await loadCategories()}catch(e){setError(String(e));return}for(const e of imported){const key=`${e.type}::${e.username}`.toLowerCase();const old=existing.get(key);if(old&&e.username){if(conflictChoice[e.id]==="replace")xs=xs.map(x=>x.id===old.id?{...e,id:old.id,seq:old.seq}:x);else continue}else{const max=Math.max(0,...xs.map(x=>x.seq||0));xs=[...xs,{...e,seq:max+1}]}}const ok=await saveEntries(renumber(xs));if(!ok)return;setImported([]);setDialog("none")};
 const createCategory=async()=>{const n=categoryName.trim();if(!n){setError("请输入分类名称");return}const parent=categoryParent;try{await invoke("category_create",{name:n,icon:categoryIcon,parentName:parent});await loadCategories();setCategoryName("");setCategoryParent(null);setDialog("none")}catch(e){setError(String(e))}};
 const openBackupSettings=async()=>{if(dirty){askDiscard(openBackupSettings);return}setError("");await loadBackupSettings();setDialog("backupSettings")};
 const chooseBackupDirectory=async()=>{try{const p=await open({multiple:false,directory:true});if(typeof p==="string")setBackupDirectory(p)}catch(e){setError(String(e))}};
 const saveBackupSettings=async()=>{setError("");try{const bs=await invoke<BackupSettings>("backup_settings_set",{enabled:backupEnabled,directory:backupDirectory||null,retention:backupRetention});setBackupSettings(bs);if(bs.enabled){const first=await invoke<BackupSettings>("backup_now");setBackupSettings(first);setSaveState("自动备份已启用")}else{setSaveState("自动备份已关闭")};setDialog("none")}catch(e){setError(String(e))}};
 const backupNow=async()=>{setError("");try{const bs=await invoke<BackupSettings>("backup_now");setBackupSettings(bs);setBackupEnabled(bs.enabled);setBackupDirectory(bs.directory||"");setBackupRetention(bs.retention||5);setSaveState("已创建加密版本备份")}catch(e){setError(String(e))}};
 const backup=async()=>{try{const p=await save({defaultPath:"LocalVault-完整备份.vault",filters:[{name:"LocalVault 加密备份",extensions:["vault"]}]});if(p){setPendingFile(p);setCredentialKind("backup");setCredentialPassword("");setDialog("credential")}}catch(e){setError(String(e))}};
 const loadDiagnostics=async()=>{try{const s=await invoke<DiagnosticsSettings>("diagnostics_settings_get");setDiagnosticsEnabled(s.enabled)}catch{}};
 const toggleDiagnostics=async()=>{try{const s=await invoke<DiagnosticsSettings>("diagnostics_settings_set",{enabled:!diagnosticsEnabled});setDiagnosticsEnabled(s.enabled);setSaveState(s.enabled?"故障日志已启用":"故障日志已关闭并清理")}catch(e){setError(String(e))}};
 const openUpdateDialog=()=>{if(updateInstalling)return;setUpdateInfo(null);setUpdateVersions([]);setUpdateError("");setUpdateChecking(false);setUpdateChecked(false);setUpdateConfirming(false);setUpdateProgress(0);setUpdateTotal(0);setDialog("update")};
 const loadUpdateVersions=async()=>{try{const r=await fetch("https://api.github.com/repos/Tenderne1/LocalVault/releases");if(!r.ok)return;const j=await r.json();const list=(Array.isArray(j)?j:[]).map((rel:any)=>({tag:String(rel.tag_name||""),date:String(rel.published_at||""),body:String(rel.body||"").trim(),assets:(rel.assets||[]).map((a:any)=>({name:String(a.name||""),url:String(a.browser_download_url||"")})).filter((x:{name:string;url:string})=>x.name&&x.url)})).filter((x:UpdateVer)=>x.tag&&versionGt(x.tag,APP_VERSION));setUpdateVersions(list)}catch{}};
const checkUpdate=async()=>{if(updateChecking||updateInstalling)return;setUpdateInfo(null);setUpdateVersions([]);setUpdateError("");setUpdateProgress(0);setUpdateTotal(0);setUpdateChecked(false);setUpdateChecking(true);try{const u=await check({timeout:15000});setUpdateInfo(u);setUpdateChecked(true);setUpdateCheckedAt(now());void loadUpdateVersions()}catch(e){setUpdateError("检测更新失败："+String(e))}finally{setUpdateChecking(false)}};
 const installUpdate=async()=>{if(!updateInfo||updateInstalling)return;setUpdateConfirming(false);setUpdateInstalling(true);setUpdateError("");setUpdateProgress(0);setUpdateTotal(0);try{let downloaded=0;let contentLength=0;await updateInfo.downloadAndInstall(event=>{if(event.event==="Started"){contentLength=event.data.contentLength||0;setUpdateTotal(contentLength);setUpdateProgress(0)}else if(event.event==="Progress"){downloaded+=event.data.chunkLength;setUpdateProgress(contentLength?Math.min(100,Math.round(downloaded/contentLength*100)):0)}else if(event.event==="Finished"){setUpdateProgress(100)}})}catch(e){setUpdateError("下载或安装更新失败："+String(e));setUpdateInstalling(false)}};
 const installVersion=async(v:UpdateVer)=>{if(updateInstalling)return;setUpdateError("");setUpdateConfirming(false);setUpdateInfo(null);try{const u=await check({timeout:15000,target:v.tag.replace(/^v/i,"")});setUpdateInfo(u);setUpdateConfirming(true)}catch(e){setUpdateError(`未找到版本 ${v.tag.replace(/^v/i,"")} 的签名更新包：${String(e)}`);setUpdateChecked(false)}};
 const isExpired=(e:Entry)=>e.expiresAt!==null&&e.expiresAt<=now();
 const expiring=(e:Entry)=>e.expiresAt!==null&&e.expiresAt>now()&&e.expiresAt<=now()+7*86400000;
 const isReused=(e:Entry)=>!!e.passwordReused;
 const securityMatches=(e:Entry)=>securityFilter==="all"||securityFilter==="weak"&&(e.passwordScore??0)<40||securityFilter==="reused"&&isReused(e)||securityFilter==="expired"&&isExpired(e)||securityFilter==="expiring"&&expiring(e)||securityFilter==="untagged"&&e.tags.length===0;
 const filtered=entries.filter(entryVisible);
 const sorted=sortMode==="name"?[...filtered].sort((a,b)=>a.name.localeCompare(b.name,"zh")):sortMode==="updated"?[...filtered].sort((a,b)=>b.updatedAt-a.updatedAt):filtered;
 const current=entries.find(e=>e.id===selected);
 const weak=entries.filter(e=>(e.passwordScore??0)<40).length;
 const reused=entries.filter(isReused).length;
 const expired=entries.filter(isExpired).length;
 const expiringSoon=entries.filter(expiring).length;
 const backupDue=!backupSettings.lastBackupAt||now()-backupSettings.lastBackupAt>30*86400000;
 const setDraftField=(k:keyof Entry,v:any)=>{if(!draft)return;setDraft({...draft,[k]:v});setDirty(true)};
 const targetHref=(v:string)=>{const s=v.trim();if(/^https?:\/\//i.test(s))return s;if(/^www\./i.test(s))return `https://${s}`;if(/^\d{1,3}(?:\.\d{1,3}){3}(?::\d+)?(?:\/|$)/.test(s)||/^localhost(?::\d+)?(?:\/|$)/i.test(s))return `http://${s}`;return `https://${s}`};
 const openSavedUrl=async(v:string)=>{try{const url=targetHref(v);if(browserChoice.useDefault){await openUrl(url)}else{await invoke("open_url_in_browser",{url,browser:browserChoice.path||null})}}catch(e){setError("无法打开网址："+String(e))}};
 const openGeneratorForEditor=()=>{setGeneratorPick(()=>{const pick=(pw:string)=>{setDraftField("password",pw)};return pick});setDialog("generator")};
 const configureBrowser=async()=>{setError("");const detected=await invoke<BrowserInfo[]>("detect_browsers").catch(()=>[]);setDetectedBrowsers(detected);setBrowserUseDefaultDraft(browserChoice.useDefault);setBrowserPathDraft(browserChoice.path);setDialog("browserSettings")};
 const saveBrowserChoice=()=>{const path=browserPathDraft.trim();if(!browserUseDefaultDraft&&(!path||!path.toLowerCase().endsWith(".exe"))){setError("请填写有效的浏览器 .exe 完整路径；也可以勾选使用系统默认浏览器。");return}const next={useDefault:browserUseDefaultDraft,path:browserUseDefaultDraft?"":path};setBrowserChoice(next);localStorage.setItem("lv_browser_choice",JSON.stringify(next));setDialog("none");setError("")};

 const categoryTree=useMemo(()=>{const byParent=new Map<string|null,Category[]>();for(const c of categories){const key=c.parentName||null;if(!byParent.has(key))byParent.set(key,[]);byParent.get(key)!.push(c)}const out:{cat:Category;depth:number;hasChildren:boolean}[]=[];const walk=(parent:string|null,depth:number)=>{for(const c of byParent.get(parent)||[]){const hasChildren=(byParent.get(c.name)||[]).length>0;out.push({cat:c,depth,hasChildren});if(hasChildren&&!collapsedCats.includes(c.name))walk(c.name,depth+1)}};walk(null,0);return out},[categories,collapsedCats]);
 const categoryOptions=useMemo(()=>{const byParent=new Map<string|null,Category[]>();for(const c of categories){const key=c.parentName||null;if(!byParent.has(key))byParent.set(key,[]);byParent.get(key)!.push(c)}const out:{cat:Category;depth:number}[]=[];const walk=(parent:string|null,depth:number)=>{for(const c of byParent.get(parent)||[]){out.push({cat:c,depth});walk(c.name,depth+1)}};walk(null,0);return out},[categories]);
 const categoryCount=(name:string)=>entries.filter(e=>descendantNames(name).includes(e.category)).length;
 const validParents=(editing:string|null)=>categoryOptions.filter(({cat})=>cat.name!==(editing||"")&&!descendantNames(editing||"").includes(cat.name));
 const credentialModal=dialog==="credential"?<Modal title={credentialKind==="backup"?"验证本机 Vault 主密码":credentialKind==="export"?"验证本机 Vault 主密码":credentialKind==="import"?"验证原电脑 Vault 主密码":credentialKind==="restore"?"验证备份主密码":credentialKind==="verify"?"验证备份主密码":"验证当前主密码"}><p className="muted">{credentialKind==="backup"?"完整备份属于最高敏感级别操作。导出前必须再次验证当前 Vault 主密码；验证成功后才会复制完整 Vault 数据。主密码不会写入备份文件。":credentialKind==="export"?"为了生成仅账号密码的加密数据，需要再次验证当前 Vault 主密码。主密码不会写入导出文件。":credentialKind==="import"?"这是跨电脑导入的安全验证：只有知道原电脑 Vault 主密码，才能解开导出文件中的数据密钥。主密码不会保存到新电脑。":credentialKind==="restore"?"导入完整备份前先验证备份对应的 Vault 主密码；验证失败不会覆盖当前 Vault。":credentialKind==="verify"?"校验只读取备份文件，不会修改当前 Vault；需要输入该备份对应的主密码。":"修改密保或主密码前，需要再次验证当前 Vault 主密码。验证通过后你可以选择只修改密保、只修改主密码，或同时修改。"}</p><input autoFocus type="password" value={credentialPassword} placeholder="Vault 主密码" onChange={e=>setCredentialPassword(e.target.value)} onKeyDown={e=>e.key==="Enter"&&void confirmCredential()}/>{error&&<div className="error">{error}</div>}<div className="modalActions"><button className="cancelBtn" onClick={()=>{setDialog("none");setPendingFile(null);setCredentialPassword("");setBackupData(null);setError("")}}>取消</button><button onClick={confirmCredential}>验证并继续</button></div></Modal>:null;

 if(view==="loading")return <div className="splash"><div className="logo">🔐</div><h1>LocalVault</h1><p>正在初始化…</p></div>;
 if(view==="create")return <><Auth title="创建你的 Vault" subtitle="首次启动仅在本机创建加密 Vault。"><input autoFocus type="password" placeholder="主密码（至少 8 位，含数字/大小写字母/特殊符号）" value={master} onChange={e=>setMaster(e.target.value)}/><input type="password" placeholder="再次输入主密码" value={confirmMaster} onChange={e=>setConfirmMaster(e.target.value)}/><Meter value={score(master)}/><button onClick={createVault}>创建 Vault</button><button className="secondary" onClick={restore}>恢复已有 Vault</button>{error&&<div className="error">{error}</div>}</Auth>{credentialModal}</>;
 if(view==="recoverySetup")return <Auth title="设置恢复方式" subtitle="保存离线 Recovery Code，并设置三组本地问题。"><div className="codebox">{generatedRecoveryCode}</div><button className="secondary" onClick={()=>void copyRecoveryCode(generatedRecoveryCode)}>复制 Recovery Code</button>{questions.map((q,i)=><label className="left" key={i}>问题 {i+1}<input value={q} onChange={e=>{const x=[...questions];x[i]=e.target.value;setQuestions(x)}}/><input value={answers[i]} placeholder="答案（不能为空，可使用中文）" onChange={e=>{const x=[...answers];x[i]=e.target.value;setAnswers(x)}}/></label>)}<button onClick={finishRecovery}>保存恢复设置并进入</button><button className="secondary" onClick={()=>setView("vault")}>稍后设置</button>{error&&<div className="error">{error}</div>}</Auth>;
 if(view==="unlock"){const remaining=unlockSecurity.lockUntil?Math.max(0,unlockSecurity.lockUntil-Date.now()):0;return <Auth title="解锁 LocalVault" subtitle="请输入主密码解锁本地加密数据库。"><input autoFocus type="password" placeholder="主密码" value={master} onChange={e=>setMaster(e.target.value)} onKeyDown={e=>e.key==="Enter"&&!remaining&&unlock()} disabled={!!remaining}/>{unlockSecurity.captchaRequired&&unlockSecurity.captchaSvg&&<div className="captchaBox"><div className="captchaRow"><img src={`data:image/svg+xml;charset=utf-8,${encodeURIComponent(unlockSecurity.captchaSvg)}`} alt="图片验证码"/><button className="secondary captchaRefresh" type="button" onClick={refreshCaptcha} disabled={!!remaining}>换一张</button></div><input value={captchaInput} onChange={e=>setCaptchaInput(e.target.value.toUpperCase())} placeholder="请输入图片中的验证码" maxLength={6} onKeyDown={e=>e.key==="Enter"&&!remaining&&unlock()} disabled={!!remaining}/><small className="muted">连续输错 3 次后需要验证码。</small></div>}{remaining>0&&<div className="lockout"><b>主密码暂时锁定</b><span>已触发防暴力破解保护，请等待 {Math.max(1,Math.ceil(remaining/1000))} 秒后再试。</span></div>}{unlockSecurity.failedAttempts>0&&remaining===0&&<div className="muted">当前连续失败 {unlockSecurity.failedAttempts} 次；连续 5 次锁定 30 秒，10 次锁定 5 分钟，15 次锁定 30 分钟，20 次锁定 1 小时。</div>}<button onClick={unlock} disabled={!!remaining}>解锁</button><button className="link recoveryLink" onClick={openRecoveryUnlock}>找回主密码</button>{remaining>0&&<small className="recoveryHint">主密码锁定不会影响“找回主密码”。Recovery Code 和密保验证仍可使用。</small>}{error&&<div className="error">{error}</div>}</Auth>}
 if(view==="recoveryUnlock")return <Auth title="找回主密码" subtitle="使用 Recovery Code + 三组密保验证身份。验证通过后，请设置新的主密码。"><input value={recoveryCode} onChange={e=>setRecoveryCode(e.target.value)} placeholder="Recovery Code"/>{questions.map((q,i)=><label className="left" key={i}>Q{i+1}. {q}<input value={answers[i]} onChange={e=>{const x=[...answers];x[i]=e.target.value;setAnswers(x)}}/></label>)}<button onClick={verifyRecovery}>验证身份</button><button className="secondary" onClick={async()=>{await invoke("recovery_cancel").catch(()=>{});setView("unlock")}}>返回</button>{error&&<div className="error">{error}</div>}</Auth>;
 if(view==="recoveryReset")return <Auth title="设置新主密码" subtitle="验证已通过。请输入新的主密码，修改完成后必须重新使用新密码验证。"><input autoFocus type="password" placeholder="新主密码（至少 8 位，含数字/大小写字母/特殊符号）" value={newMaster} onChange={e=>setNewMaster(e.target.value)}/><input type="password" placeholder="再次输入新主密码" value={newConfirm} onChange={e=>setNewConfirm(e.target.value)}/><Meter value={score(newMaster)}/><button onClick={resetMasterFromRecovery}>修改主密码</button><button className="secondary" onClick={cancelRecoveryReset}>取消</button>{error&&<div className="error">{error}</div>}</Auth>;
 if(view==="about")return <div className="about"><div className="aboutCard"><div className="logo">🔐</div><h1>LocalVault</h1><p>本地优先密码保险库</p><dl><dt>版本</dt><dd>V{APP_VERSION} · 自动登录填充版</dd><dt>数据库</dt><dd>SQLite</dd><dt>KDF</dt><dd>Argon2id</dd><dt>AEAD</dt><dd>XChaCha20-Poly1305</dd></dl><label className="checkRow diagRow"><input type="checkbox" checked={diagnosticsEnabled} onChange={()=>void toggleDiagnostics()}/> 启用故障日志{diagnosticsEnabled&&<small className="muted">（用于排查问题，记录操作日志）</small>}</label><button onClick={()=>setView("vault")}>返回</button></div></div>;

 return <div className="app" onContextMenu={e=>{const t=e.target as HTMLElement;if(!t.closest("input,textarea,button,.editor,.context")){e.preventDefault();setMenu({x:e.clientX,y:e.clientY,kind:"blank"})}}} onClick={e=>{setMenu(null);if(editMode)return;const t=e.target as HTMLElement;if(t.closest(".editor,.item,aside,header,input,button,select,textarea,label,.context,.overlay,.modal,.resizeHandle"))return;setSelected(null);setDraft(null);setEditMode(false);setDirty(false)}}>
  <header><b>🔐 LocalVault <span className="pill">V{APP_VERSION} · 自动登录填充版</span></b><input ref={searchRef} className="topsearch" value={query} onChange={e=>setQuery(e.target.value)} placeholder="搜索名称、账号、网址、标签…"/><span className="status">{saveState}</span>{backupDue&&<button className="ghost backupReminder" title="建议创建加密备份" onClick={openBackupSettings}>⚠ 建议备份</button>}<button className="ghost" title={theme==="dark"?"切换到浅色主题":"切换到深色主题"} onClick={()=>setTheme(theme==="dark"?"light":"dark")}>{theme==="dark"?"☀️":"🌙"}</button><button className="ghost" onClick={lock}>🔒 锁定</button></header>
  {dialog==="none"&&error&&<div className="toastError" role="alert"><span>{error}</span><button type="button" title="关闭提示" onClick={()=>setError("")}>✕</button></div>}
  <aside style={{width:sidebarWidth}}><button className="new" onClick={add}>＋ 新建密码</button><p className={category==="全部"?"active":""} onClick={()=>requestAction(()=>setCategory("全部"))}>全部 <span>{entries.length}</span></p><p className={category==="收藏"?"active":""} onClick={()=>requestAction(()=>setCategory("收藏"))}>★ 收藏 <span>{entries.filter(e=>e.favorite).length}</span></p><hr/><small>分类</small>{categoryTree.map(({cat:c,depth,hasChildren})=><p key={c.name} data-category-name={c.name} style={{paddingLeft:9+depth*20}} className={`categoryItem ${category===c.name?"active":""} ${draggedCategory===c.name?"categoryDragging":""} ${dragOverCategory===c.name?"categoryDragOver":""}`} onPointerDown={ev=>{if(ev.button!==0)return;const t=ev.target as HTMLElement;if(t.closest("button,input,select,textarea"))return;if(ev.detail>=2&&hasChildren){pointerDrag.current=null;return}if(dirty){setError("请先保存或放弃当前编辑，再调整分类顺序");return}pointerDrag.current={kind:"category",id:c.name,startX:ev.clientX,startY:ev.clientY,active:false,didDrag:false}}} onClick={ev=>{if(suppressClick.current){suppressClick.current=false;ev.preventDefault();ev.stopPropagation();return}requestAction(()=>setCategory(c.name))}} onDoubleClick={ev=>{if(!hasChildren)return;ev.preventDefault();ev.stopPropagation();suppressClick.current=true;toggleCategory(c.name);setTimeout(()=>{suppressClick.current=false},0)}} onContextMenu={ev=>{ev.preventDefault();ev.stopPropagation();setMenu({x:ev.clientX,y:ev.clientY,kind:"category",name:c.name})}}><span className="catIcon">{c.icon}</span>{hasChildren&&<span className="catToggle" onClick={ev=>{ev.stopPropagation();toggleCategory(c.name)}}>{collapsedCats.includes(c.name)?"▸":"▾"}</span>}{c.name}<span>{categoryCount(c.name)}</span></p>)}<button className="utility" onClick={()=>{setCategoryName("");setCategoryIcon("📁");setCategoryParent(null);setDialog("category")}}>＋ 新建分类</button><button className="utility" onClick={()=>requestAction(()=>{loadTrash();setDialog("trash")})}>♻ 回收站 <span>{trash.length}</span></button><hr/>
   <div className="sideGroup"><button className="groupHeader" onClick={()=>setCollapsedGroups(a=>({...a,security:!a.security}))}>🛡️ 安全中心 <span>{collapsedGroups.security?"▸":"▾"}</span></button>{!collapsedGroups.security&&<div className="groupBody"><p className={securityFilter==="weak"?"securityActive":""} onClick={()=>setSecurityFilter(securityFilter==="weak"?"all":"weak")}>弱密码 <span>{weak}</span></p><p className={securityFilter==="reused"?"securityActive":""} onClick={()=>setSecurityFilter(securityFilter==="reused"?"all":"reused")}>重复密码 <span>{reused}</span></p><p className={securityFilter==="expired"?"securityActive":""} onClick={()=>setSecurityFilter(securityFilter==="expired"?"all":"expired")}>过期密码 <span>{expired}</span></p><p className={securityFilter==="expiring"?"securityActive":""} onClick={()=>setSecurityFilter(securityFilter==="expiring"?"all":"expiring")}>7天内到期 <span>{expiringSoon}</span></p><p className={securityFilter==="untagged"?"securityActive":""} onClick={()=>setSecurityFilter(securityFilter==="untagged"?"all":"untagged")}>未设置标签 <span>{entries.filter(e=>!e.tags.length).length}</span></p><label>自动锁定<select value={autoLock} onChange={e=>setAutoLock(+e.target.value)}><option value={1}>1 分钟</option><option value={5}>5 分钟</option><option value={10}>10 分钟</option><option value={30}>30 分钟</option></select></label></div>}</div>
   <div className="sideGroup">
    <button className="groupHeader" onClick={() => setCollapsedGroups(a => ({ ...a, data: !a.data }))}>📦 数据管理 <span>{collapsedGroups.data ? "▸" : "▾"}</span></button>
    {!collapsedGroups.data && <div className="groupBody">
     <button className="utility" title="完整复制当前 Vault 文件；恢复时需要原 Vault 主密码。" onClick={backup}>完整备份</button>
     <button className="utility" title="只导出密码条目，可在另一台电脑导入；导出时需要原 Vault 主密码。" onClick={exportEntries}>仅导出账号密码</button>
     <button className="utility" title="导入另一台电脑导出的 LVX 加密数据，或从 CSV 模板批量导入；导入时需要原电脑主密码。" onClick={()=>{setError("");setBulkDemoStatus("");setDialog("importMethod")}}>导入账号密码</button>
     <button className="utility" title="恢复完整 Vault 备份；会先验证主密码，并可选择合并或替换当前 Vault。" onClick={() => requestAction(restore)}>导入完整备份</button>
     <button className="utility" title="设置 Vault 保存后的自动版本备份目录和保留代数，并可立即创建一份已验证的加密版本备份。备份目录必须位于 LocalVault 数据目录之外。" onClick={openBackupSettings}>安全备份设置</button>
     {backupSettings.enabled && <small className={backupSettings.lastError ? "dangerText" : "muted"}>{backupSettings.lastError ? "⚠ 自动备份失败" : "✓ 自动备份已启用"}</small>}
    </div>}
   </div>
   <div className="sideGroup"><button className="groupHeader" onClick={()=>setCollapsedGroups(a=>({...a,system:!a.system}))}>⚙️ 系统 <span>{collapsedGroups.system?"▸":"▾"}</span></button>{!collapsedGroups.system&&<div className="groupBody"><button className="utility" onClick={openSecuritySettings}>密保及密码修改</button><button className="utility" onClick={()=>{setGeneratorPick(null);setDialog("generator")}}>密码生成器</button><button className="utility" onClick={configureBrowser}>浏览器打开设置</button><button className="utility" onClick={()=>void openGeneralSettings()}>常规设置</button><button className="utility" onClick={()=>setShowOnboarding(true)}>快速上手教程</button><button className="utility" onClick={()=>void openAutofillDialog()}>🌐 浏览器填充</button><button className="utility" onClick={openUpdateDialog}>软件更新</button><button className="utility" onClick={()=>requestAction(()=>setView("about"))}>关于</button></div>}</div></aside>
  <main style={{left:sidebarWidth,gridTemplateColumns:`${listWidth}px 7px minmax(0,1fr)`}} onClick={()=>setMenu(null)}>
   <section className="list">
    <div className="listToolbar"><label className="selectAll"><input type="checkbox" checked={filtered.length>0&&filtered.every(e=>multiSelected.includes(e.id))} onChange={ev=>setMultiSelected(ev.target.checked?Array.from(new Set([...multiSelected,...filtered.map(e=>e.id)])):multiSelected.filter(id=>!filtered.some(e=>e.id===id)))} /> 全选</label><span>{filtered.length} 条记录</span><label className="sortSelect" title="列表排序方式（自动排序时不可拖动）"><select value={sortMode} onChange={e=>setSortMode(e.target.value as "manual"|"name"|"updated")}><option value="manual">手动排序</option><option value="name">按名称</option><option value="updated">按更新时间</option></select></label>{securityFilter!=="all"&&<button className="secondary" onClick={()=>setSecurityFilter("all")}>清除安全筛选</button>}{multiSelected.length>0&&<><b>已选 {multiSelected.length}</b><button onClick={()=>void batchFavorite(true)}>★ 批量收藏</button><button onClick={()=>void batchFavorite(false)}>☆ 取消收藏</button><button onClick={batchAddTag}>🏷 批量加标签</button><button className="dangerBtn" onClick={batchDelete}>批量删除</button></>}</div>
    {sorted.map(e=><div key={e.id} data-entry-id={e.id} onPointerDown={ev=>{if(ev.button!==0)return;const t=ev.target as HTMLElement;if(t.closest("input,button,select,textarea,.starBtn"))return;if(dirty){setError("请先保存或放弃当前编辑，再调整密码顺序");return}if(sortMode!=="manual"){setError("当前为自动排序，拖动调整已禁用；请先在上方切换为手动排序");return} pointerDrag.current={kind:"entry",id:e.id,startX:ev.clientX,startY:ev.clientY,active:false,didDrag:false}}} className={`item ${selected===e.id?"selected":""} ${expiring(e)?"expiring":""} ${draggedId===e.id?"dragging":""} ${dragOverId===e.id?"dragOver":""}`} onClick={ev=>{if(suppressClick.current){suppressClick.current=false;ev.preventDefault();ev.stopPropagation();return}choose(e.id)}} onContextMenu={ev=>{ev.preventDefault();ev.stopPropagation();setMenu({x:ev.clientX,y:ev.clientY,kind:"entry",id:e.id})}}>
      <div className="itemMain"><input className="selectBox" type="checkbox" checked={multiSelected.includes(e.id)} onChange={ev=>{ev.stopPropagation();setMultiSelected(a=>ev.target.checked?[...a,e.id]:a.filter(x=>x!==e.id))}} onClick={ev=>ev.stopPropagation()}/><div className="dragGrip" title="按住拖动排序">⋮⋮</div><div className="seq">#{e.seq}</div><div><b>{e.name||"未命名"}</b><span>{e.nickname||e.username||e.email||"无账号信息"}</span><small>{e.type} · {e.category}</small></div></div>
      <div className="itemMeta">{e.passwordEncrypted&&<button className="listCopyBtn" title="复制密码" onClick={ev=>{ev.stopPropagation();void listCopyPassword(e)}}>{listCopied===e.id?"✓":"📋"}</button>}<button className={`starBtn ${e.favorite?"isFavorite":""}`} title={e.favorite?"取消收藏":"添加到收藏"} onClick={ev=>{ev.stopPropagation();if(dirty)askDiscard(()=>toggleFavorite(e.id));else void toggleFavorite(e.id)}}>{e.favorite?"★":"☆"}</button>{(e.passwordScore??0)<40&&<i title="弱密码" className="weakMark">⚠</i>}{expiring(e)&&<i title="7天内过期">⚠</i>}<em>{e.passwordScore??0}</em></div>
    </div>)}{!filtered.length&&<div className="empty">暂无密码条目<br/><small>在空白区域右键添加</small></div>}
   </section><div className="resizeHandle listResize" title="拖动调整密码列表宽度" onMouseDown={ev=>{ev.preventDefault();ev.stopPropagation();setResizing("list")}}/>
   <section className="editor">{draft?<Editor entry={draft} entries={entries} categories={categories} categoryOptions={categoryOptions} types={types} tags={tags} editMode={editMode} setEditMode={setEditMode} dirty={dirty} setDirty={setDirty} showPass={showPass} setShowPass={setShowPass} onChange={setDraftField} onSave={commitDraft} onCancel={()=>{clearRevealed();const original=entries.find(e=>e.id===draft.id);if(original)setDraft({...original});else{setSelected(null);setDraft(null)}setEditMode(false);setDirty(false)}} onDelete={()=>setDialog("delete")} onHistory={()=>{loadHistory(draft.id);setDialog("history")}} targetHref={targetHref} onOpenUrl={openSavedUrl} onAddType={openTypePrompt} onAddTag={openTagPrompt} onOpenGenerator={openGeneratorForEditor} revealed={revealed} onReveal={(e)=>{void revealPassword(e)}} onError={setError}/>:<div className="editor-empty"><div className="editor-empty-icon">🔐</div><h2>选择一个密码</h2><p>点击密码后查看详情；点击空白区域可关闭右侧详情。</p></div>}</section>
  </main>
  <div className="resizeHandle sidebarResize" title="拖动调整左侧分类栏宽度" style={{left:sidebarWidth}} onMouseDown={ev=>{ev.preventDefault();setResizing("sidebar")}}/>
  {dragPoint&&(draggedId||draggedCategory)&&<div className="dragPreview" style={{left:dragPoint.x+14,top:dragPoint.y+14}}>{draggedId?<>↕ {entriesRef.current.find(e=>e.id===draggedId)?.name||"未命名"}</>:<>↕ {categoriesRef.current.find(c=>c.name===draggedCategory)?.icon} {draggedCategory}</>}</div>}
  {menu&&<div ref={menuRef} className="context" style={{left:menu.x,top:menu.y}} onClick={e=>e.stopPropagation()}>{menu.kind==="entry"?<><strong>密码操作</strong><button onClick={()=>{choose(menu.id!);setMenu(null)}}>查看</button><button onClick={()=>{if(dirty)askDiscard(async()=>{performChoose(menu.id!);setEditMode(true)});else{performChoose(menu.id!);setEditMode(true)}setMenu(null)}}>✎ 编辑</button><hr/><button onClick={()=>void copyEntryInfo()}>⧉ 复制当前条目</button><button onClick={()=>void copyEntryField("username")}>👤 复制账号</button><button onClick={()=>void copyEntryField("password")}>🔑 复制密码</button><button onClick={()=>void copyEntryField("url")}>🔗 复制网址</button><hr/><button onClick={()=>{if(dirty)askDiscard(()=>toggleFavorite(menu.id!));else void toggleFavorite(menu.id!);setMenu(null)}}>{entries.find(e=>e.id===menu.id)?.favorite?"☆ 取消收藏":"★ 添加到收藏"}</button><hr/><button onClick={()=>{if(dirty)askDiscard(()=>moveEntry(menu.id!,-1));else void moveEntry(menu.id!,-1);setMenu(null)}}>↑ 上移</button><button onClick={()=>{if(dirty)askDiscard(()=>moveEntry(menu.id!,1));else void moveEntry(menu.id!,1);setMenu(null)}}>↓ 下移</button><button onClick={()=>{if(dirty)askDiscard(()=>moveEntry(menu.id!,"top"));else void moveEntry(menu.id!,"top");setMenu(null)}}>⬆ 置顶</button><hr/><button className="dangerBtn" onClick={()=>{const action=()=>{setSelected(menu.id!);setDraft(entries.find(e=>e.id===menu.id!)||null);setEditMode(false);setDirty(false);setDialog("delete")};if(dirty)askDiscard(action);else action();setMenu(null)}}>删除</button></>:menu.kind==="category"?<><strong>分类操作</strong><button onClick={()=>{setCategory(menu.name||"全部");setMenu(null)}}>查看</button><button onClick={()=>{setCategoryName("");setCategoryIcon("📁");setCategoryParent(menu.name||null);setDialog("category");setMenu(null)}}>＋ 新建子分类</button><button onClick={()=>{const c=categories.find(x=>x.name===menu.name);if(c){setCategoryEdit(c);setCategoryName(c.name);setCategoryIcon(c.icon);setCategoryParent(c.parentName);setDialog("categoryEdit")}setMenu(null)}}>编辑分类</button><hr/><button onClick={()=>{void moveCategory(menu.name||"",-1);setMenu(null)}}>↑ 上移</button><button onClick={()=>{void moveCategory(menu.name||"",1);setMenu(null)}}>↓ 下移</button><button onClick={()=>{void moveCategory(menu.name||"","top");setMenu(null)}}>⬆ 置顶</button><hr/><button className="dangerBtn" onClick={()=>{deleteCategory(menu.name||"");setMenu(null)}}>删除分类</button></>:<><strong>快速操作</strong><button onClick={add}>＋ 添加密码</button></>}</div>}
  {dialog==="category"&&<Modal title="新建分类"><div className="catPreview"><span>{categoryIcon}</span><b>{categoryName.trim()||"新分类"}</b></div><label>分类名称<input autoFocus value={categoryName} onChange={e=>setCategoryName(e.target.value)} placeholder="例如：工作、金融、社交"/></label><label>父分类<select value={categoryParent||""} onChange={e=>setCategoryParent(e.target.value||null)}><option value="">顶级分类</option>{categoryOptions.map(({cat:c,depth})=><option key={c.name} value={c.name}>{"　".repeat(depth)}{c.icon} {c.name}</option>)}</select></label><label>选择图标</label><div className="iconGrid">{CATEGORY_ICONS.map(g=><div key={g.group} className="iconGroup"><small>{g.group}</small><div className="iconGridRow">{g.icons.map(i=><button type="button" key={i} className={categoryIcon===i?"active":""} onClick={()=>setCategoryIcon(i)}>{i}</button>)}</div></div>)}</div>{error&&<div className="error">{error}</div>}<div className="modalActions"><button className="cancelBtn" onClick={()=>{setDialog("none");setCategoryParent(null)}}>取消</button><button onClick={createCategory}>创建</button></div></Modal>}
  {dialog==="delete"&&<Modal title="移入回收站">{multiSelected.length>0?<p>确定将已选的 {multiSelected.length} 条密码移入回收站吗？</p>:<p>确定删除“{current?.name||"此密码"}”吗？</p>}<p className="muted">密码会先移入回收站，可在回收站中恢复或永久删除。</p><div className="modalActions"><button className="cancelBtn" onClick={()=>setDialog("none")}>取消</button><button className="dangerBtn" onClick={multiSelected.length?applyBatchDelete:deleteCurrent}>移入回收站</button></div></Modal>}
  {dialog==="discard"&&<Modal title="检测到未保存修改"><p>当前密码有未保存的修改。</p><p className="muted">切换密码、锁定或离开前，请选择是否保存。</p><div className="modalActions"><button className="cancelBtn" onClick={()=>setDialog("none")}>继续编辑</button><button className="secondary" onClick={()=>{const a=pendingAction;setPendingAction(null);setDialog("none");setDraft(null);setSelected(null);setEditMode(false);setDirty(false);void a?.()}}>放弃修改</button><button onClick={async()=>{const ok=await commitDraft();if(!ok)return;const a=pendingAction;setPendingAction(null);setDialog("none");void a?.()}}>保存修改</button></div></Modal>}
  {dialog==="history"&&<Modal title="修改记录"><p className="muted">仅记录哪些字段发生过变化，不保存字段的新旧具体内容。</p>{history.length?<div className="historyList">{history.map((h,i)=><div key={i}><b>{new Date(h.changedAt).toLocaleString()}</b><span>{h.fields.join("、")}</span></div>)}</div>:<p>暂无修改记录。</p>}<div className="modalActions"><button onClick={()=>setDialog("none")}>关闭</button></div></Modal>}
  {credentialModal}
  {dialog==="backupSettings"&&<Modal title="安全备份设置"><p><b>自动版本备份</b>会在每次 Vault 成功保存后，向独立目录写入一份经过解密校验的加密 Vault 副本，并按保留代数自动清理旧版本。</p><p className="muted">备份文件本身仍是加密 Vault，不保存主密码。备份目录不能位于 <code>LocalVault</code> 数据目录内部。若要抵御勒索软件，建议选择不同磁盘、断开连接的外置盘，或具有版本历史/回收能力的同步目录。</p><label className="checkRow"><input type="checkbox" checked={backupEnabled} onChange={e=>setBackupEnabled(e.target.checked)}/> 启用自动版本备份</label><div className="backupSettingRow"><label className="backupDirectoryField"><span>备份目录</span><div className="backupDirectoryControl"><input readOnly value={backupDirectory} placeholder="请选择独立的备份文件夹"/><button className="secondary" onClick={()=>void chooseBackupDirectory()}>选择文件夹</button></div></label><label className="backupRetentionField"><span>保留版本</span><select value={backupRetention} onChange={e=>setBackupRetention(+e.target.value)}><option value={1}>1 份</option><option value={3}>3 份</option><option value={5}>5 份</option><option value={10}>10 份</option><option value={20}>20 份</option><option value={50}>50 份</option></select></label></div>{backupSettings.lastBackupAt&&<p className="muted">最近备份：{new Date(backupSettings.lastBackupAt).toLocaleString()}</p>}{backupSettings.lastError&&<div className="error">最近一次自动备份失败：{backupSettings.lastError}</div>}{error&&<div className="error">{error}</div>}<div className="modalActions"><button className="cancelBtn" onClick={()=>{setDialog("none");setError("");setBulkDemoStatus("")}}>取消</button><button className="secondary" title="立即创建一份已验证的加密版本备份。" onClick={()=>void backupNow()}>立即备份</button><button onClick={()=>void saveBackupSettings()}>保存并验证</button></div></Modal>}

  {dialog==="securitySettings"&&<Modal title="密保及密码修改"><p className="muted">当前主密码验证通过。请选择要修改的内容；未勾选的项目保持不变。</p><label className="checkRow"><input type="checkbox" checked={changeMaster} onChange={e=>setChangeMaster(e.target.checked)}/> 修改主密码</label>{changeMaster&&<><input autoFocus type="password" placeholder="新主密码（至少 8 位，含数字/大小写字母/特殊符号）" value={newMaster} onChange={e=>setNewMaster(e.target.value)}/><input type="password" placeholder="再次输入新主密码" value={newConfirm} onChange={e=>setNewConfirm(e.target.value)}/><Meter value={score(newMaster)}/></>}<label className="checkRow"><input type="checkbox" checked={changeRecovery} onChange={e=>setChangeRecovery(e.target.checked)}/> 修改密保（会自动生成新的 Recovery Code）</label>{changeRecovery&&<div className="securitySettingsFields">{securityQuestions.map((q,i)=><label className="left" key={i}>问题 {i+1}<input value={q} onChange={e=>{const x=[...securityQuestions];x[i]=e.target.value;setSecurityQuestions(x)}}/><input value={securityAnswers[i]} placeholder="答案（不能为空，可使用中文）" onChange={e=>{const x=[...securityAnswers];x[i]=e.target.value;setSecurityAnswers(x)}}/></label>)}<p className="muted">修改密保后，旧 Recovery Code 立即失效，并自动生成新的 Recovery Code。新 Recovery Code 会尝试自动复制到剪贴板。</p></div>}{error&&<div className="error">{error}</div>}<div className="modalActions"><button className="cancelBtn" onClick={()=>{setVerifiedMasterPassword("");setDialog("none")}}>取消</button><button onClick={saveSecuritySettings}>保存修改</button></div></Modal>}
  {dialog==="backupRestore"&&<Modal title="导入完整备份"><p>备份验证成功：包含 <b>{backupData?.entries.length||0}</b> 条密码、<b>{backupData?.categories.length||0}</b> 个分类。</p><p className="muted">现在可以选择恢复方式。<b>合并</b>不会删除当前 Vault 的新数据；同一条目 ID 已存在时保留当前条目，备份中的新条目会追加。缺少的分类会一并加入。</p>{error&&<div className="error">{error}</div>}<div className="modalActions"><button className="cancelBtn" onClick={()=>{setDialog("none");setPendingFile(null);setCredentialPassword("");setBackupData(null);setError("")}}>取消</button><button className="secondary" title="选择另一个 .vault 文件并验证其完整性。" onClick={()=>void verifyBackupFile()}>校验备份文件</button><button className="secondary" onClick={()=>void restoreBackupMerge()}>合并到当前 Vault</button><button className="dangerBtn" onClick={()=>void restoreBackupReplace()}>替换当前 Vault</button></div></Modal>}
  {dialog==="importMethod"&&<Modal title="导入账号密码"><p className="muted">选择导入方式：</p><div className="importMethodBtns"><button className="secondary" title="导入另一台电脑导出的 LVX 加密数据；需要原电脑主密码。" onClick={()=>void importEntries()}>导入账号密码文件（.lvx）</button><button title="从 CSV 模板批量导入大量设备/系统账号密码。模板为明文文件，导入完成后建议立即删除。" onClick={()=>{setBulkDemoStatus("");setDialog("bulkImport")}}>从 CSV 模板批量导入</button></div>{error&&<div className="error">{error}</div>}<div className="modalActions"><button className="cancelBtn" onClick={()=>{setDialog("none");setError("")}}>取消</button></div></Modal>}
  {dialog==="bulkImport"&&<Modal title="批量导入模板"><p><b>适用于大批量设备、系统、平台账号密码导入。</b></p><p className="muted">先下载 CSV 模板，在 Excel/WPS 中批量填写后，再选择“从模板批量导入”。模板自带 2 条演示数据；可直接保留，系统会自动识别并忽略，也可以在填写前整行删除。支持名称、账号、密码、网址/IP/APP、类型、分类、邮箱、手机号、昵称、标签、收藏、密码有效期和备注。密码有效期固定为“永不过期”“30天”“90天”“180天”“365天”。</p><div className="bulkColumns"><b>必要列：</b>名称、账号/用户名（密码可留空）<br/><b>可选列：</b>网址/IP/APP名称、账号类型、分类、邮箱、手机号、平台昵称、标签、收藏、密码有效期、备注</div><p className="dangerText">安全提示：CSV 模板属于明文文件，不会被 LocalVault 加密；LocalVault 只把字段作为文本导入。模板示例数据会被自动忽略，不会写入 Vault。导入成功后请及时删除或妥善保护原 CSV。</p><p className="muted">编码：支持 UTF-8、UTF-8 BOM、UTF-16 以及 GBK/GB18030，Excel/WPS 直接保存的常见中文 CSV 也可导入。</p>{bulkDemoStatus&&<div className="success">{bulkDemoStatus}</div>}{error&&<div className="error">{error}</div>}<div className="modalActions"><button className="cancelBtn" onClick={()=>{setDialog("none");setError("");setBulkDemoStatus("")}}>取消</button><button className="secondary" onClick={()=>void downloadBulkTemplate()}>下载模板</button><button onClick={()=>void importBulkTemplate()}>从模板批量导入</button></div></Modal>}
  {dialog==="import"&&<Modal title="导入账号密码"><p>共检测到 <b>{imported.length}</b> 条待导入记录。文件只在本机处理，不会保存源文件主密码。</p><p className="muted">重复账号按“类型 + 用户名”判断。默认遇到重复项选择“跳过”；如需覆盖，可在下方切换为“替换”。</p><div className="historyList">{imported.slice(0,100).map(e=>{const key=`${e.type}::${e.username}`.toLowerCase();const conflict=entries.some(x=>x.username&&e.username&&`${x.type}::${x.username}`.toLowerCase()===key);return <div key={e.id}><b>#{e.seq} · {e.name}</b><span>{e.type} · {e.username||"无用户名"}{conflict&&<> <select value={conflictChoice[e.id]||"skip"} onChange={ev=>setConflictChoice(a=>({...a,[e.id]:ev.target.value as "replace"|"skip"}))}><option value="skip">跳过</option><option value="replace">替换</option></select></>}</span></div>})}</div>{imported.length>100&&<p className="muted">记录较多，仅预览前 100 条；其余记录仍会正常参与导入。</p>}<div className="modalActions"><button className="cancelBtn" onClick={()=>setDialog("none")}>取消</button><button onClick={applyImport}>应用导入</button></div></Modal>}
  {dialog==="trash"&&<Modal title="回收站"><p className="muted">删除的密码会在 {trashDays} 天后自动清理。恢复后会回到当前 Vault。</p>{trash.length?<div className="historyList">{trash.map(e=><div key={e.id}><b>#{e.seq} · {e.name||"未命名"}</b><span>{e.type} · {e.username||e.email||"无账号信息"}<span className="trashActions"><button onClick={()=>restoreTrash(e.id)}>恢复</button><button className="dangerBtn" onClick={()=>purgeTrash(e.id)}>彻底删除</button></span></span></div>)}</div>:<p>回收站为空。</p>}<label>自动保留<select value={trashDays} onChange={async e=>{const v=+e.target.value;setTrashDays(v);try{setTrash(await invoke<Entry[]>("trash_list",{retentionDays:v}))}catch{}}}><option value={7}>7 天</option><option value={14}>14 天</option><option value={21}>21 天</option><option value={30}>30 天</option></select></label><div className="modalActions"><button className="cancelBtn" onClick={()=>setDialog("none")}>关闭</button><button className="dangerBtn" onClick={()=>purgeTrash()}>清空回收站</button></div></Modal>}
  {dialog==="categoryEdit"&&<Modal title="编辑分类"><div className="catPreview"><span>{categoryIcon}</span><b>{categoryName.trim()||categoryEdit?.name}</b></div><label>分类名称<input autoFocus value={categoryName} onChange={e=>setCategoryName(e.target.value)} placeholder="分类名称"/></label><label>父分类<select value={categoryParent||""} onChange={e=>setCategoryParent(e.target.value||null)}><option value="">顶级分类</option>{validParents(categoryEdit?.name||null).map(({cat:c,depth})=><option key={c.name} value={c.name}>{"　".repeat(depth)}{c.icon} {c.name}</option>)}</select></label><label>选择图标</label><div className="iconGrid">{CATEGORY_ICONS.map(g=><div key={g.group} className="iconGroup"><small>{g.group}</small><div className="iconGridRow">{g.icons.map(i=><button type="button" key={i} className={categoryIcon===i?"active":""} onClick={()=>setCategoryIcon(i)}>{i}</button>)}</div></div>)}</div>{error&&<div className="error">{error}</div>}<div className="modalActions"><button className="cancelBtn" onClick={()=>setDialog("none")}>取消</button><button onClick={updateCategory}>保存</button></div></Modal>}
  {dialog==="browserSettings"&&<Modal title="浏览器打开设置"><p className="muted">打开密码条目中的网址时，只会传递网址，不会传递账号、密码或其他敏感信息。</p><label className="checkRow"><input type="checkbox" checked={browserUseDefaultDraft} onChange={e=>setBrowserUseDefaultDraft(e.target.checked)}/> 使用系统默认浏览器</label>{!browserUseDefaultDraft&&<><label>浏览器程序路径<input value={browserPathDraft} onChange={e=>setBrowserPathDraft(e.target.value)} placeholder="例如 C:\Program Files\Google\Chrome\Application\chrome.exe"/></label>{detectedBrowsers.length>0?<div className="browserList"><b>已检测到的浏览器：</b>{detectedBrowsers.map(b=><button key={b.path} className="secondary browserBtn" onClick={()=>setBrowserPathDraft(b.path)}>{b.icon?<img className="browserIcon" src={`data:image/png;base64,${b.icon}`} alt=""/>:null}<span>{b.name}</span><small>{b.path}</small></button>)}</div>:<p className="muted">未检测到常见浏览器。请手动填写浏览器 exe 完整路径。</p>}<p className="muted">只允许 http/https 网址；不会自动登录或自动填充密码。</p></>}{error&&<div className="error">{error}</div>}<div className="modalActions"><button className="cancelBtn" onClick={()=>{setDialog("none");setError("")}}>取消</button><button onClick={saveBrowserChoice}>保存设置</button></div></Modal>}{dialog==="autofill"&&<Modal title="🌐 浏览器填充"><div className="autofillBody"><div className="autofillRow"><div className="autofillText"><b>启用填充服务</b><p className="muted">开启一次后：解锁 Vault 自动连接、锁定自动断开，无需反复开关；首次用配对码配对后，之后解锁即可直接填充，无需重新配对。（服务仅监听 127.0.0.1，密码仅在点击填充瞬间下发。）Vault 锁定状态下开关不可用。</p></div><label className={`switch ${autofill.vaultUnlocked?"":"disabled"}`} title={autofill.vaultUnlocked?"点击切换填充服务":"Vault 未解锁，无法开启填充服务"}><input type="checkbox" checked={autofill.enabled} disabled={!autofill.vaultUnlocked} onChange={e=>void toggleAutofill(e.target.checked)}/><span className="slider"/></label></div><div className="statusBox"><span className={autofill.running?"dotOn":"dotOff"}>●</span> 服务状态：{autofill.running?"运行中":"未运行"}　|　端口：{autofill.port}　|　已配对设备：{autofill.pairedCount} 台</div>{autofill.lastError&&<div className="error">{autofill.lastError}</div>}{autofill.running&&<div className="autofillPair"><button className="secondary" onClick={()=>void genPairCode()}>生成 6 位配对码（首次配对用）</button>{pairCode&&<div className="pairCode"><div className="pairRow"><b>配对码：{pairCode}</b><button className="secondary" onClick={()=>void copyText(pairCode,false)}>📋 复制</button></div><p className="muted">5 分钟内有效，且只能使用一次。</p></div>}<button className="secondary" onClick={()=>void unpairAll()}>断开所有配对设备</button></div>}<div className="autofillSteps"><b>使用步骤</b><ol><li>解锁 Vault，打开上方开关（仅首次需要手动打开；之后解锁会自动连接、锁定自动断开）。</li><li>首次使用：点击“生成 6 位配对码”。</li><li>在 Edge/Chrome 中加载 LocalVault-Fill 扩展（解压扩展目录），打开扩展弹窗输入配对码完成配对。</li><li>之后无需再配对：只要 Vault 处于解锁状态，访问登录页点击密码框旁的 🔑 按钮即可自动填充。</li></ol></div></div><div className="modalActions"><button className="cancelBtn" onClick={()=>setDialog("none")}>关闭</button></div></Modal>}{dialog==="generalSettings"&&<Modal title="常规设置"><div className="autofillRow"><div className="autofillText"><b>开机自动启动</b><p className="muted">登录 Windows 后自动启动 LocalVault（默认关闭）。开机自启不会绕过主密码，密码数据仍由主密码保护。</p></div><label className="switch"><input type="checkbox" checked={autostart} onChange={e=>void toggleAutostart(e.target.checked)}/><span className="slider"/></label></div><div className="settingsGroup"><b>唤出程序快捷键（全局）</b><p className="muted">在任何界面按组合键即可唤出 LocalVault 窗口（类似微信）。默认 Ctrl+Alt+L，点击「录制」可更换；需包含 Ctrl 或 Alt。</p><div className="shortcutRow"><span>🌐 全局唤出</span><code>{globalHotkey.replace("Control","Ctrl")}</code>{shortcutRecording==="global"?<b className="recording">按下组合键…</b>:<button className="secondary" onClick={()=>{setError("");setGlobalHotkeyErr("");setShortcutRecording("global")}}>录制</button>}</div>{globalHotkeyErr&&<div className="error">{globalHotkeyErr}</div>}<label className="checkRow"><input type="checkbox" checked={globalHotkeyEnabled} onChange={e=>void toggleGlobalHotkey(e.target.checked)}/> 启用全局唤出快捷键</label></div><div className="settingsGroup"><b>应用内快捷键</b><p className="muted">点击「录制」后按下新的组合键（需包含 Ctrl 或 Alt）。「Esc 关闭菜单 / 弹窗」为固定快捷键。</p><div className="shortcutRow"><span>🔍 搜索</span><code>{shortcuts.search.replace("Control","Ctrl")}</code>{shortcutRecording==="search"?<b className="recording">按下组合键…</b>:<button className="secondary" onClick={()=>{setError("");setShortcutRecording("search")}}>录制</button>}</div><div className="shortcutRow"><span>💾 保存</span><code>{shortcuts.save.replace("Control","Ctrl")}</code>{shortcutRecording==="save"?<b className="recording">按下组合键…</b>:<button className="secondary" onClick={()=>{setError("");setShortcutRecording("save")}}>录制</button>}</div><div className="shortcutRow"><span>🔒 锁定</span><code>{shortcuts.lock.replace("Control","Ctrl")}</code>{shortcutRecording==="lock"?<b className="recording">按下组合键…</b>:<button className="secondary" onClick={()=>{setError("");setShortcutRecording("lock")}}>录制</button>}</div><div className="settingsActions"><button className="secondary" onClick={()=>{setShortcuts({...DEFAULT_SHORTCUTS});setError("")}}>恢复默认</button></div></div><div className="closePrefBox"><b>点击窗口 × 的关闭行为</b><p className="muted">{closePref.remember?`当前已记住：点击 × 时${closePref.action==="tray"?"始终最小化到系统托盘":"始终退出程序"}。`:"当前未记住：每次点击 × 都会询问（最小化到托盘 / 退出）。"}选择“最小化到系统托盘”后，程序继续在后台运行，可从托盘图标恢复；只有托盘右键“退出”才会真正结束程序。</p><button className="secondary" onClick={resetClosePref}>清除记住的选择，恢复每次询问</button></div>{error&&<div className="error">{error}</div>}<div className="modalActions"><button className="cancelBtn" onClick={()=>{setDialog("none");setShortcutRecording(null);setError("")}}>关闭</button></div></Modal>}{closeChoiceOpen&&<Modal title="关闭 LocalVault"><p className="muted">你希望点击 × 后 LocalVault 如何运行？</p><label className="checkRow"><input type="radio" checked={closeChoiceAction==="tray"} onChange={()=>setCloseChoiceAction("tray")}/> 最小化到系统托盘（后台继续运行，可从托盘恢复）</label><label className="checkRow"><input type="radio" checked={closeChoiceAction==="quit"} onChange={()=>setCloseChoiceAction("quit")}/> 退出程序</label><label className="checkRow"><input type="checkbox" checked={closeRemember} onChange={e=>setCloseRemember(e.target.checked)}/> 记住我的选择，下次点击 × 不再询问</label><div className="modalActions"><button className="cancelBtn" onClick={()=>setCloseChoiceOpen(false)}>取消</button><button onClick={()=>void applyCloseChoice()}>确定</button></div></Modal>}{dialog==="generator"&&<GeneratorModal onClose={()=>{setGeneratorPick(null);setDialog("none")}} onPick={generatorPick||undefined}/>}{dialog==="update"&&<Modal title="软件更新">
 <p className="muted">当前版本：<b>{APP_VERSION}</b></p>
 <p className="muted">打开这个窗口不会联网。只有你点击下面的“检测更新”，LocalVault 才会访问 GitHub Releases 读取版本信息与更新说明，不会上传 Vault、账号、密码或 Recovery Code。</p>
 <div className="updateInfo"><h3>本版本更新内容（{APP_VERSION}）</h3><ul className="notesList">{RELEASE_NOTES.map(n=><li key={n}>{n}</li>)}</ul></div>
 {updateChecking&&<p>正在检测 GitHub Releases…</p>}
 {!updateChecking&&!updateChecked&&!updateError&&updateCheckedAt===null&&<p className="muted">尚未检测更新。只有点击下方“检测更新”后才会联网，你可以先看完上面的更新内容再决定。</p>}
 {!updateChecking&&updateChecked&&!updateError&&updateVersions.length===0&&!updateInfo&&<p className="successText">✓ 当前已是最新版本。</p>}
 {updateVersions.length>0&&<div className="updateList"><h3>发现 {updateVersions.length} 个新版本（点击版本查看该版本的更新内容）</h3>{updateVersions.map(v=>{const open=expandedVer===v.tag;return <div className="updateInfo" key={v.tag}><button className="versionRow" onClick={()=>setExpandedVer(open?null:v.tag)}><b>版本 {v.tag.replace(/^v/i,"")}</b><span className={`verArrow ${open?"open":""}`}>▾</span></button>{open&&<div className="versionDetail"><p className="muted">发布时间：{v.date?new Date(v.date).toLocaleString():"未知"}</p><p className="muted">更新内容：</p>{v.body?<div className="updateNotes">{v.body}</div>:<div className="updateNotes muted">该版本暂未提供更新说明，请确认发布来源后再决定是否安装。</div>}<div className="verActions"><button className="primary" onClick={()=>void installVersion(v)} disabled={updateInstalling}>⬇ 软件内自动更新</button></div>{v.assets.length>0&&<div className="directDownload"><p className="muted">下载方式：复制链接到浏览器或 IDM 下载，也可手动下载：</p>{v.assets.map(a=><div key={a.name} className="dlRow"><input readOnly value={a.url} onFocus={e=>e.currentTarget.select()}/><button className="secondary" onClick={()=>void copyText(a.url,false)}>复制到浏览器下载</button></div>)}<p className="muted"><button className="link" onClick={()=>void openUrl(`https://github.com/Tenderne1/LocalVault/releases/tag/${v.tag}`)}>手动下载（打开该版本发布页）</button></p></div>}</div>}</div>})}</div>}
 {updateInfo&&<><p className="muted">也可用浏览器打开 <button className="link" onClick={()=>void openUrl("https://github.com/Tenderne1/LocalVault/releases")}>GitHub Releases 页面</button> 下载安装版或便携版。</p><p className="muted">LocalVault 不会在检测更新时自动下载。只有你明确点击“确认下载并安装”后才会开始下载。</p>{updateConfirming&&!updateInstalling&&<div className="updateConfirm"><p><b>确认下载并安装最新版 {updateInfo.version}？</b></p><p className="muted">安装过程可能会关闭并重启 LocalVault。请先保存正在编辑的条目，并确保已有可用备份。</p><div className="modalActions"><button className="cancelBtn" onClick={()=>setUpdateConfirming(false)}>取消</button><button onClick={()=>void installUpdate()}>确认下载并安装</button></div></div>}{updateInstalling&&<div className="updateProgress"><div className="progressTrack"><i style={{width:`${updateTotal?updateProgress:100}%`}}/></div><span>{updateTotal?`${updateProgress}%`:'正在下载…'}</span></div>}</>}
 <div className="updateInfo pluginDl"><h3>浏览器填充插件（扩展）下载</h3><p className="muted">浏览器扩展独立于主程序更新，请从蓝奏云下载最新版插件包：</p><div className="dlRow"><input readOnly value="https://wwbak.lanzoub.com/b01n4i035g" onFocus={e=>e.currentTarget.select()}/><button className="secondary" onClick={()=>void copyText("https://wwbak.lanzoub.com/b01n4i035g",false)}>复制链接</button></div><p className="muted">访问密码：<b>7fy9</b><button className="link" onClick={()=>void openUrl("https://wwbak.lanzoub.com/b01n4i035g")}>打开蓝奏云页面</button></p></div>
 {!updateChecking&&updateCheckedAt!==null&&<p className="muted">上次检测时间：{new Date(updateCheckedAt).toLocaleString()}</p>}
 {updateError&&<div className="error">{updateError}</div>}
 {!updateChecking&&updateError&&<p className="muted">请确认网络可用，或稍后再次点击“检测更新”。LocalVault 的密码数据不会参与更新请求。</p>}
 <div className="modalActions"><button className="cancelBtn" disabled={updateInstalling} onClick={()=>{if(!updateInstalling){setDialog("none");setUpdateError("");setUpdateConfirming(false)}}}>关闭</button>{updateInfo?(!updateInstalling&&!updateConfirming&&<button onClick={()=>setUpdateConfirming(true)}>下载并安装 {updateInfo.version}</button>):(!updateInstalling&&<button disabled={updateChecking} onClick={()=>void checkUpdate()}>{updateChecking?"检测中…":"检测更新"}</button>)}</div>
</Modal>}
 
  {dialog==="prompt"&&promptMeta&&<Modal title={promptMeta.title}><input autoFocus value={promptValue} onChange={e=>setPromptValue(e.target.value)} placeholder={promptMeta.placeholder} onKeyDown={e=>e.key==="Enter"&&confirmPrompt()}/>{error&&<div className="error">{error}</div>}<div className="modalActions"><button className="cancelBtn" onClick={()=>{setPromptMeta(null);setPromptValue("");setDialog("none")}}>取消</button><button onClick={confirmPrompt}>确定</button></div></Modal>}
  {dialog==="recoveryCode"&&<Modal title="新的 Recovery Code"><p className="muted">旧 Recovery Code 已失效。新码已尝试复制到剪贴板，请立即离线保存。</p><div className="codebox">{newRecoveryCode}</div><div className="modalActions"><button className="cancelBtn" onClick={()=>{setNewRecoveryCode("");setDialog("none")}}>我已保存</button><button onClick={()=>void copyRecoveryCode(newRecoveryCode)}>重新复制</button></div></Modal>}{showOnboarding&&<OnboardingModal onClose={()=>{setShowOnboarding(false);localStorage.setItem("lv_onboarding_seen","1")}}/>} </div>
}

function OnboardingModal({onClose}:{onClose:()=>void}){
 const steps=[
  {icon:"👋",title:"欢迎使用 LocalVault",body:<ul><li><b>LocalVault</b> 是本地加密密码管理器：Vault 数据只保存在本机，主密码不会上传。</li><li>主界面分三栏：<b>左侧分类栏</b>（分类 / 收藏 / 安全中心）、<b>中间密码列表</b>、<b>右侧详情 / 编辑区</b>。</li><li>顶部搜索框可实时搜索名称、账号、网址、标签等。</li></ul>},
  {icon:"➕",title:"添加与编辑密码",body:<ul><li>点击<b>「＋ 新建密码」</b>或列表空白处右键 → 添加密码。</li><li>点击列表条目在右侧<b>查看</b>；点「✎ 编辑密码」修改后<b>保存</b>。</li><li>密码可设置<b>过期时间</b>、<b>标签</b>、<b>收藏</b>；删除的条目先进<b>回收站</b>，可恢复。</li></ul>},
  {icon:"🔍",title:"搜索、排序与批量操作",body:<ul><li>顶部搜索框实时过滤；快捷键 <b>Ctrl+F</b> 直接聚焦搜索。</li><li>列表上方「排序」可切换 <b>手动 / 按名称 / 按更新时间</b>；手动模式下可拖动排序。</li><li>勾选多条（或全选）后，可<b>批量收藏 / 取消收藏 / 加标签 / 删除</b>。</li></ul>},
  {icon:"🔑",title:"复制密码与安全",body:<ul><li>密码、账号、网址均可一键复制（列表 📋 按钮或右键菜单）。</li><li>条目右键菜单可<b>复制当前条目 / 账号 / 密码 / 网址</b>；同一平台多个账号时可复制条目后再编辑。</li><li>左侧「安全中心」可筛选 <b>弱密码 / 重复 / 过期 / 7 天内到期</b>；自动锁定时间可在安全中心设置。</li></ul>},
  {icon:"🌐",title:"浏览器自动填充",body:<ul><li>「系统 → 🌐 浏览器填充」打开总开关，点击「生成 6 位配对码」。</li><li>浏览器扩展管理页（<b>edge://extensions</b> 或 <b>chrome://extensions</b>）→「加载解压缩的扩展」选择 extension 文件夹，点扩展图标输入配对码完成配对。</li><li>配对一次后：Vault 解锁自动连接、锁定自动断开，登录页密码框旁 🔑 点击即可填充，无需反复配对。</li></ul>},
  {icon:"🛡️",title:"数据安全与系统",body:<ul><li>定期「数据管理 → <b>完整备份</b>」；建议开启<b>安全备份设置</b>（自动版本备份）。</li><li>「常规设置」可配置<b>开机自启</b>、<b>全局唤起快捷键</b>（默认 Ctrl+Alt+L）、<b>点 × 最小化到托盘</b>；右上角可切换<b>浅色 / 暗色主题</b>。</li><li>软件更新支持按版本查看说明与<b>软件内自动更新</b>；想再看本教程：随时打开 <b>系统 → 快速上手教程</b>。</li></ul>},
 ];
 const [step,setStep]=useState(0);
 const last=step===steps.length-1;
 return <div className="overlay"><div className="modal onboarding"><div className="onbHead"><span className="onbIcon">{steps[step].icon}</span><h2>{steps[step].title}</h2></div><div className="onbBody">{steps[step].body}</div><div className="onbDots">{steps.map((_,i)=><i key={i} className={i===step?"on":""}/>)}</div><div className="modalActions"><button className="cancelBtn" onClick={onClose}>跳过</button>{step>0&&<button className="secondary" onClick={()=>setStep(step-1)}>上一步</button>}{!last&&<button onClick={()=>setStep(step+1)}>下一步</button>}{last&&<button onClick={onClose}>开始使用</button>}</div></div></div>;
}
function GeneratorModal({onClose,onPick}:{onClose:()=>void;onPick?:(pw:string)=>void}){
 const [lower,setLower]=useState(true),[upper,setUpper]=useState(true),[digits,setDigits]=useState(true),[symbols,setSymbols]=useState(true);
 const [exclude,setExclude]=useState("il1LoO"),[length,setLength]=useState(16),[count,setCount]=useState(1),[results,setResults]=useState<string[]>([]),[genError,setGenError]=useState("");
 const generate=()=>{
  let pool="";if(lower)pool+="abcdefghijklmnopqrstuvwxyz";if(upper)pool+="ABCDEFGHIJKLMNOPQRSTUVWXYZ";if(digits)pool+="0123456789";if(symbols)pool+="!@#$%^&*()-_=+[]{};:,.?";
  pool=Array.from(pool).filter(c=>!exclude.includes(c)).join("");
  if(!pool){setGenError("至少选择一种字符类型，并确保排除字符后仍有可用字符。");return}
  const out:string[]=[];
  for(let n=0;n<count;n++){const a=new Uint32Array(length);crypto.getRandomValues(a);out.push(Array.from(a,v=>pool[v%pool.length]).join(""))}
  setResults(out);
 };
 const genCopy=async(v:string)=>{try{await invoke("copy_secure",{text:v});scheduleClipboardClear();return}catch{}try{await navigator.clipboard.writeText(v)}catch{}};
 return <Modal title="密码生成器"><p className="muted">使用系统安全随机数生成密码。默认排除容易混淆的字符。</p>
  <div className="generatorGrid"><label><input type="checkbox" checked={lower} onChange={e=>setLower(e.target.checked)}/> a-z</label><label><input type="checkbox" checked={upper} onChange={e=>setUpper(e.target.checked)}/> A-Z</label><label><input type="checkbox" checked={digits} onChange={e=>setDigits(e.target.checked)}/> 0-9</label><label><input type="checkbox" checked={symbols} onChange={e=>setSymbols(e.target.checked)}/> !@#$%</label></div>
  <label>排除字符<input value={exclude} onChange={e=>setExclude(e.target.value)} placeholder="例如 il1LoO"/></label>
  <div className="generatorRow"><label>密码长度<select value={length} onChange={e=>setLength(+e.target.value)}>{Array.from({length:61},(_,i)=>i+8).map(n=><option key={n} value={n}>{n} 位</option>)}</select></label><label>密码数量<select value={count} onChange={e=>setCount(+e.target.value)}>{Array.from({length:10},(_,i)=>i+1).map(n=><option key={n} value={n}>{n} 个</option>)}</select></label></div>
  <button onClick={generate}>生成密码</button>
  {genError&&<div className="error">{genError}</div>}
  {results.length>0&&<div className="generatorResults">{results.map((r,i)=><div key={i} className="generatorResult"><code>{r}</code>{onPick&&<button className="secondary" onClick={()=>{onPick(r);onClose()}}>填入</button>}<button className="secondary" onClick={()=>{void genCopy(r)}}>复制</button></div>)}</div>}
  <div className="modalActions"><button className="cancelBtn" onClick={onClose}>关闭</button><button onClick={generate}>重新生成</button></div>
 </Modal>
}

function Editor({entry,entries,categories,categoryOptions,types,tags,editMode,setEditMode,dirty,setDirty,showPass,setShowPass,onChange,onSave,onDelete,onHistory,targetHref,onOpenUrl,onAddType,onAddTag,onOpenGenerator,onCancel,revealed,onReveal,onError}:{entry:Entry;entries:Entry[];categories:Category[];categoryOptions:{cat:Category;depth:number}[];types:string[];tags:string[];editMode:boolean;setEditMode:(x:boolean)=>void;dirty:boolean;setDirty:(x:boolean)=>void;showPass:boolean;setShowPass:(x:boolean)=>void;onChange:(k:keyof Entry,v:any)=>void;onSave:()=>Promise<boolean>;onDelete:()=>void;onHistory:()=>void;targetHref:(v:string)=>string;onOpenUrl:(v:string)=>Promise<void>;onAddType:()=>void;onAddTag:()=>void;onOpenGenerator:()=>void;onCancel:()=>void;revealed:{id:string;text:string}|null;onReveal:(e:Entry)=>void;onError:(m:string)=>void}){
 const readOnly=!editMode;const days=entry.expiresAt?Math.max(0,Math.ceil((entry.expiresAt-now())/86400000)):0;
 const canOpenUrl=(v:string)=>/^https?:\/\//i.test(v)||/^www\./i.test(v)||/^\d{1,3}(?:\.\d{1,3}){3}(?::\d+)?(?:\/|$)/.test(v)||/^[a-z0-9.-]+\.[a-z]{2,}(?:\/|$)/i.test(v);
 const [copied,setCopied]=useState<string|null>(null);
 const copyField=async(kind:string,text:string)=>{if(!text)return;if(kind==="password"&&!editMode){try{await invoke("vault_copy_password",{entryId:entry.id,passwordEncrypted:text});setCopied(kind);setTimeout(()=>{setCopied(c=>c===kind?null:c)},1500)}catch(err){onError(String(err))}return}const ok=await copyText(text,false);if(!ok)return;setCopied(kind);setTimeout(()=>{setCopied(c=>c===kind?null:c)},1500)};


 const setExpiry=(v:string)=>{if(v==="never")onChange("expiresAt",null);else if(v==="30")onChange("expiresAt",expiryDate(30));else if(v==="90")onChange("expiresAt",expiryDate(90));else if(v==="180")onChange("expiresAt",expiryDate(180));else if(v==="365")onChange("expiresAt",expiryDate(365));};
 return <div className="form"><div className="formTop"><div className="titleBlock"><span>#{entry.seq} · {entry.type}</span><input className="title" readOnly={readOnly} value={entry.name} onChange={e=>onChange("name",e.target.value)}/></div><div className="formActions">{!editMode&&<button onClick={()=>setEditMode(true)}>✎ 编辑密码</button>}{editMode&&<><button className="secondary" onClick={onCancel}>取消</button><button onClick={onSave}>保存</button></>}<button className="dangerBtn" onClick={onDelete}>删除</button></div></div>
  <div className="readonlyBanner">{readOnly?"只读查看模式：不会因为浏览而修改密码。":"编辑模式：修改完成后请点击“保存”。"}{dirty&&<b> · 有未保存修改</b>}</div>
  <label>账号类型<div className="inlineControl"><select disabled={readOnly} value={entry.type} onChange={e=>onChange("type",e.target.value)}>{types.map(t=><option key={t}>{t}</option>)}</select>{editMode&&<button onClick={onAddType}>＋ 自定义类型</button>}</div></label>
  <div className="sectionTitle">登录凭据</div><div className={`credentialGrid ${editMode?"isEditing":"isReadonly"}`}><label>账号/用户名<div className="inlineControl"><input readOnly={readOnly} value={entry.username} onChange={e=>onChange("username",e.target.value)}/>{entry.username&&<button className="passCopyBtn" onClick={()=>void copyField("username",entry.username)}>{copied==="username"?"已复制":"复制"}</button>}</div></label><label>密码<div className="passwordField"><div className="passrow">{editMode?<><input type={showPass?"text":"password"} value={entry.password} onChange={e=>onChange("password",e.target.value)} placeholder="留空则保持原密码"/><button className="passEyeBtn" title={showPass?"隐藏密码":"显示输入的密码"} onClick={()=>setShowPass(!showPass)}>👁</button><button className="passGenerateBtn" title="打开密码生成器，生成后可一键填入" onClick={onOpenGenerator}>生成</button><button className="passCopyBtn" onClick={()=>void copyField("password",entry.password)}>{copied==="password"?"已复制":"复制"}</button></>:<><input readOnly type={revealed?.id===entry.id?"text":"password"} value={revealed?.id===entry.id?revealed.text:""} placeholder="••••••••"/><button className="passEyeBtn" title="查看密码（20 秒后自动隐藏）" onClick={()=>onReveal(entry)}>👁</button>{entry.passwordEncrypted&&<button className="passCopyBtn" onClick={()=>void copyField("password",entry.passwordEncrypted??"")}>{copied==="password"?"已复制":"复制"}</button>}</>}</div><PasswordStrength value={editMode?score(entry.password):(entry.passwordScore??0)}/></div></label></div>
  <div className="sectionTitle bindingTitle">绑定信息</div><div className="fieldGrid"><label>邮箱<input readOnly={readOnly} value={entry.email} onChange={e=>onChange("email",e.target.value)}/></label><label>手机号<input readOnly={readOnly} value={entry.phone} onChange={e=>onChange("phone",e.target.value)}/></label><label>平台昵称<input readOnly={readOnly} value={entry.nickname} onChange={e=>onChange("nickname",e.target.value)}/></label></div>
  <div className="sectionTitle otherTitle">其他信息</div><label>网址 / IP / APP 名称<div className="urlrow"><input readOnly={readOnly} value={entry.url} placeholder="https://example.com / 192.168.1.1 / App 名称" onChange={e=>onChange("url",e.target.value)}/>{entry.url&&<button className="passCopyBtn" onClick={()=>void copyField("url",entry.url)}>{copied==="url"?"已复制":"复制"}</button>}{entry.url&&canOpenUrl(entry.url)&&<button onClick={()=>void onOpenUrl(entry.url)}>↗ 打开浏览器</button>}</div></label>
  <div className="fieldGrid"><label>分类<select disabled={readOnly} value={entry.category||"默认"} onChange={e=>onChange("category",e.target.value)}>{categoryOptions.map(({cat:c,depth})=><option key={c.name} value={c.name}>{"　".repeat(depth)}{c.icon} {c.name}</option>)}</select></label><label>收藏<input type="checkbox" disabled={readOnly} checked={entry.favorite} onChange={e=>onChange("favorite",e.target.checked)} /> ★ 收藏此密码</label><label>标签<div className="inlineControl"><input readOnly={readOnly} value={entry.tags.join("、")} placeholder="工作、高频使用、财务相关" onChange={e=>onChange("tags",e.target.value.split(/[、,，]/).map(x=>x.trim()).filter(Boolean))}/>{editMode&&<button onClick={onAddTag}>＋ 自定义标签</button>}</div>{editMode&&<div className="tagChoices">{tags.map(t=><button key={t} className={entry.tags.includes(t)?"tagActive":""} onClick={()=>onChange("tags",entry.tags.includes(t)?entry.tags.filter(x=>x!==t):[...entry.tags,t])}>{entry.tags.includes(t)?"✓ ":""}{t}</button>)}</div>}</label></div>
  <div className="securityBox"><h3>时间信息</h3><div className="timeGrid"><div><span>最后修改时间</span><b>{new Date(entry.updatedAt).toLocaleString()}</b></div><div><span>密码状态</span><b className={entry.expiresAt&&days<=7?"dangerText":""}>{entry.expiresAt?days===0?"已过期":`${days} 天后过期`:"永不过期"}</b></div></div><label>密码过期设置<select disabled={readOnly} value={entry.expiresAt===null?"never":days===30?"30":days===90?"90":days===180?"180":days===365?"365":"custom"} onChange={e=>setExpiry(e.target.value)}><option value="never">永不过期</option><option value="30">30 天</option><option value="90">90 天</option><option value="180">180 天</option><option value="365">365 天</option><option value="custom">自定义（请使用日期）</option></select></label>{editMode&&<label>自定义到期日期<input type="date" value={dateInput(entry.expiresAt)} onChange={e=>onChange("expiresAt",e.target.value?new Date(e.target.value+"T23:59:59").getTime():null)}/></label>} {entry.expiresAt&&<small className={days<=7?"dangerText":"muted"}>提前 7 天提醒：{days<=7?"已触发，列表已标红":"未触发"}</small>}</div>
  <label>备注<textarea readOnly={readOnly} value={entry.notes} onChange={e=>onChange("notes",e.target.value)}/></label><div className="historyAction"><button className="secondary" onClick={onHistory}>查看最近 3 次修改记录</button></div>
 </div>
}
function Auth({title,subtitle,children}:{title:string;subtitle:string;children:React.ReactNode}){return <div className="auth"><div className="authCard"><div className="logo">🔐</div><h1>{title}</h1><p>{subtitle}</p>{children}</div></div>}
function Meter({value}:{value:number}){return <><div className="meter"><i style={{width:`${value}%`}}/></div><small>密码评分：{value}/100</small></>}
function PasswordStrength({value}:{value:number}){const label=value<=0?"未设置":value<40?"弱":value<60?"中等":value<80?"强":"超强";return <div className={`passwordStrength strength-${label}`}><span>密码强度：</span><b>{label}</b></div>}
function Modal({title,children}:{title:string;children:React.ReactNode}){return <div className="overlay"><div className="modal"><h2>{title}</h2>{children}</div></div>}
createRoot(document.getElementById("root")!).render(<App/>);
