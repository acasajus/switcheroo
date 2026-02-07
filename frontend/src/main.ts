import './style.css'

// --- Types ---
interface Game {
  name: string;
  path: string;
  relative_path: string;
  size: number;
  format: string;
  title_id?: string;
  version?: string;
  category: string; // "Base", "Update", "DLC"
  image_url?: string;
}

interface GroupedGame {
    title: string;
    image_url?: string;
    files: Game[];
    totalSize: number;
}

interface Download {
  id: string;
  filename: string;
  total_size: number;
  bytes_sent: number;
  speed: number;
}

interface ScanStatus {
    type: "scan";
    status: "scanning" | "complete" | "image_updated";
    count: number;
}

interface DownloadUpdate {
    type: "downloads";
    data: Record<string, Download>;
}

type SSEMessage = ScanStatus | DownloadUpdate;

// --- State ---
let games: Game[] = [];
let groupedGames: GroupedGame[] = [];
let activeDownloads: Record<string, Download> = {};
let scanStatus: ScanStatus | null = null;
let showConnectionModal = false;
let serverInfo: { ips: string[], port: number, webdav_enabled: boolean, webdav_auth: boolean } | null = null;

const app = document.querySelector<HTMLDivElement>('#app')!;

// --- Helpers ---
function formatBytes(bytes: number, decimals = 2) {
    if (!+bytes) return '0 B'
    const k = 1024
    const dm = decimals < 0 ? 0 : decimals
    const sizes = ['B', 'KB', 'MB', 'GB', 'TB']
    const i = Math.floor(Math.log(bytes) / Math.log(k))
    return `${parseFloat((bytes / Math.pow(k, i)).toFixed(dm))} ${sizes[i]}`
}

function getFormatColor(format: string) {
    switch (format.toLowerCase()) {
        case 'nsp': return 'text-red-400 border-red-500/30 bg-red-500/10';
        case 'nsz': return 'text-orange-400 border-orange-500/30 bg-orange-500/10';
        case 'xci': return 'text-blue-400 border-blue-500/30 bg-blue-500/10';
        case 'xcz': return 'text-cyan-400 border-cyan-500/30 bg-cyan-500/10';
        default: return 'text-slate-300 bg-slate-700/50';
    }
}

function getCategoryColor(category: string) {
    switch (category.toLowerCase()) {
        case 'base': return 'text-emerald-400 bg-emerald-500/10 border-emerald-500/20';
        case 'update': return 'text-sky-400 bg-sky-500/10 border-sky-500/20';
        case 'dlc': return 'text-pink-400 bg-pink-500/10 border-pink-500/20';
        default: return 'text-slate-400 bg-slate-500/10 border-slate-500/20';
    }
}

function groupGames(games: Game[]): GroupedGame[] {
    const groups: Record<string, GroupedGame> = {};

    games.forEach(game => {
        if (!groups[game.name]) {
            groups[game.name] = {
                title: game.name,
                image_url: game.image_url, // Take the first found image
                files: [],
                totalSize: 0
            };
        }
        // If the group doesn't have an image but this file does, use it
        if (!groups[game.name].image_url && game.image_url) {
             groups[game.name].image_url = game.image_url;
        }
        
        groups[game.name].files.push(game);
        groups[game.name].totalSize += game.size;
    });

    // Sort files within groups (Base first, then Update, then DLC)
    Object.values(groups).forEach(group => {
        group.files.sort((a, b) => {
            const catOrder = { "Base": 0, "Update": 1, "DLC": 2 };
            const catA = catOrder[a.category as keyof typeof catOrder] ?? 99;
            const catB = catOrder[b.category as keyof typeof catOrder] ?? 99;
            if (catA !== catB) return catA - catB;
            return a.relative_path.localeCompare(b.relative_path);
        });
    });

    return Object.values(groups).sort((a, b) => a.title.localeCompare(b.title));
}


// --- API ---
async function fetchInfo() {
  try {
    const response = await fetch('/api/info');
    if (response.ok) {
        serverInfo = await response.json();
    }
  } catch (error) {
    console.error('Error fetching info:', error);
  }
}

async function fetchGames() {
  try {
    const response = await fetch('/api/games');
    if (!response.ok) throw new Error('Network response was not ok');
    games = await response.json();
    groupedGames = groupGames(games);
    render();
  } catch (error) {
    console.error('Error fetching games:', error);
  }
}

function setupSSE() {
    const eventSource = new EventSource('/events');
    eventSource.onmessage = (event) => {
        try {
            const msg: SSEMessage = JSON.parse(event.data);
            if (msg.type === "downloads") {
                activeDownloads = msg.data;
                render(); 
            } else if (msg.type === "scan") {
                scanStatus = msg;
                // Don't show "image_updated" status in UI, just refresh
                if (msg.status === "image_updated") {
                     fetchGames();
                     return;
                }

                render();
                if (msg.status === "complete" || (msg.count > 0 && msg.count % 20 === 0)) {
                    fetchGames();
                }
            }
        } catch (e) {
            console.error("Error parsing SSE", e);
        }
    };
}

// --- Icons ---
const Icons = {
    Download: `<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><polyline points="7 10 12 15 17 10"/><line x1="12" y1="15" x2="12" y2="3"/></svg>`,
    Switch: `<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="2" y="6" width="20" height="12" rx="2"/><path d="M6 12h.01M18 12h.01"/></svg>`,
    Wifi: `<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M5 12.55a11 11 0 0 1 14.08 0"/><path d="M1.42 9a16 16 0 0 1 21.16 0"/><path d="M8.53 16.11a6 6 0 0 1 6.95 0"/><line x1="12" y1="20" x2="12.01" y2="20"/></svg>`,
    Close: `<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>`,
    Refresh: `<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8"/><path d="M21 3v5h-5"/><path d="M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16"/><path d="M8 16H3v5"/></svg>`,
    ImageOff: `<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="1" y1="1" x2="23" y2="23"/><path d="M21 21l-2-2m-3.268-3.268L9.5 9.5l-4-4-1.5 1.5"/><path d="M4 4l1 1"/><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><path d="M21 4H5a2 2 0 0 0-2 2v2"/></svg>`
};

// --- Components ---

const Navbar = () => `
    <nav class="sticky top-0 z-20 bg-slate-950/80 backdrop-blur-md border-b border-slate-800">
        <div class="container mx-auto px-4 h-16 flex items-center justify-between">
            <div class="flex items-center gap-2">
                <div class="text-indigo-500">${Icons.Switch}</div>
                <h1 class="text-xl font-bold bg-gradient-to-r from-indigo-400 to-purple-400 bg-clip-text text-transparent">Switcheroo</h1>
            </div>
            
            <div class="flex items-center gap-4">
                ${scanStatus ? (
                    scanStatus.status === 'scanning' ? `
                    <div class="hidden md:flex items-center gap-2 text-xs text-blue-400 bg-blue-500/10 px-3 py-1.5 rounded-full border border-blue-500/20 animate-pulse">
                        <span class="relative flex h-2 w-2">
                          <span class="animate-ping absolute inline-flex h-full w-full rounded-full bg-blue-400 opacity-75"></span>
                          <span class="relative inline-flex rounded-full h-2 w-2 bg-blue-500"></span>
                        </span>
                        Indexing: ${scanStatus.count}
                    </div>
                    ` : `
                    <div class="hidden md:flex items-center gap-2 text-xs text-emerald-400 bg-emerald-500/10 px-3 py-1.5 rounded-full border border-emerald-500/20">
                        <span class="relative flex h-2 w-2">
                          <span class="relative inline-flex rounded-full h-2 w-2 bg-emerald-500"></span>
                        </span>
                        Ready (${scanStatus.count})
                    </div>
                    `
                ) : ''}
                
                <button onclick="window.toggleModal()" class="flex items-center gap-2 px-3 py-1.5 text-sm font-medium text-slate-300 hover:text-white bg-slate-800 hover:bg-slate-700 rounded-lg transition-colors border border-slate-700">
                    ${Icons.Wifi}
                    <span class="hidden sm:inline">Connect</span>
                </button>
            </div>
        </div>
    </nav>
`;

const StatsBar = () => {
    const totalSize = games.reduce((acc, g) => acc + g.size, 0);
    const downloadCount = Object.keys(activeDownloads).length;
    const downloadSpeed = Object.values(activeDownloads).reduce((acc, d) => acc + d.speed, 0);

    return `
    <div class="grid grid-cols-2 md:grid-cols-4 gap-4 mb-8">
        <div class="bg-slate-900/50 p-4 rounded-xl border border-slate-800">
            <div class="text-slate-500 text-xs uppercase font-semibold mb-1">Total Games</div>
            <div class="text-2xl font-bold text-white">${groupedGames.length}</div>
        </div>
        <div class="bg-slate-900/50 p-4 rounded-xl border border-slate-800">
            <div class="text-slate-500 text-xs uppercase font-semibold mb-1">Collection Size</div>
            <div class="text-2xl font-bold text-white">${formatBytes(totalSize)}</div>
        </div>
        <div class="bg-slate-900/50 p-4 rounded-xl border border-slate-800">
            <div class="text-slate-500 text-xs uppercase font-semibold mb-1">Active Downloads</div>
            <div class="text-2xl font-bold text-white">${downloadCount}</div>
        </div>
        <div class="bg-slate-900/50 p-4 rounded-xl border border-slate-800">
            <div class="text-slate-500 text-xs uppercase font-semibold mb-1">Total Speed</div>
            <div class="text-2xl font-bold text-emerald-400">${formatBytes(downloadSpeed)}/s</div>
        </div>
    </div>
    `;
}

const ActiveDownloads = () => {
    const downloads = Object.values(activeDownloads);
    if (downloads.length === 0) return '';

    return `
        <div class="mb-8 bg-slate-900/50 rounded-xl border border-slate-800 overflow-hidden">
            <div class="px-4 py-3 border-b border-slate-800 bg-slate-900/80 flex items-center justify-between">
                <h3 class="font-semibold text-slate-200 text-sm">Active Downloads</h3>
                <span class="text-xs text-slate-500">${downloads.length} items</span>
            </div>
            <div class="divide-y divide-slate-800/50">
                ${downloads.map(d => {
                    const percent = d.total_size > 0 ? (d.bytes_sent / d.total_size) * 100 : 0;
                    return `
                    <div class="p-4">
                        <div class="flex justify-between mb-2">
                            <span class="font-medium text-sm text-slate-200 truncate pr-4">${d.filename}</span>
                            <span class="text-xs font-mono text-emerald-400">${formatBytes(d.speed)}/s</span>
                        </div>
                        <div class="w-full bg-slate-800 rounded-full h-2 mb-2 overflow-hidden">
                            <div class="bg-indigo-500 h-2 rounded-full transition-all duration-500" style="width: ${percent}%"></div>
                        </div>
                        <div class="flex justify-between text-xs text-slate-500">
                            <span>${formatBytes(d.bytes_sent)} of ${formatBytes(d.total_size)}</span>
                            <span>${percent.toFixed(1)}%</span>
                        </div>
                    </div>
                    `;
                }).join('')}
            </div>
        </div>
    `;
}

const GameList = () => {
    if (groupedGames.length === 0) {
        return `
            <div class="flex flex-col items-center justify-center py-20 text-slate-600">
                <div class="mb-4 text-slate-700">${Icons.Switch}</div>
                <p>No games indexed yet.</p>
                <button onclick="window.location.reload()" class="mt-4 flex items-center gap-2 text-indigo-400 hover:text-indigo-300 text-sm">
                    ${Icons.Refresh} Refresh
                </button>
            </div>
        `;
    }

    return `
        <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 2xl:grid-cols-4 gap-6">
            ${groupedGames.map(group => `
                <div class="group bg-slate-900 rounded-xl border border-slate-800 hover:border-indigo-500/30 transition-all hover:shadow-xl hover:shadow-indigo-500/10 overflow-hidden flex flex-col">
                    
                    <!-- Cover Image Area -->
                    <div class="relative aspect-video bg-slate-950 flex items-center justify-center overflow-hidden border-b border-slate-800">
                        ${group.image_url ? `
                            <img src="/files/${encodeURIComponent(group.image_url)}" alt="${group.title}" class="w-full h-full object-cover transition-transform group-hover:scale-105 duration-500">
                            <div class="absolute inset-0 bg-gradient-to-t from-slate-900 to-transparent opacity-60"></div>
                        ` : `
                            <div class="text-slate-800">${Icons.ImageOff}</div>
                            <div class="absolute inset-0 bg-gradient-to-tr from-slate-900 via-transparent to-slate-800/50"></div>
                        `}
                        
                        <div class="absolute bottom-0 left-0 p-4 w-full">
                            <h3 class="font-bold text-lg text-white leading-tight line-clamp-2 drop-shadow-md">${group.title}</h3>
                            <div class="text-xs text-slate-400 mt-1">${group.files.length} files • ${formatBytes(group.totalSize)}</div>
                        </div>
                    </div>

                    <!-- File List -->
                    <div class="flex-1 p-2 space-y-1 overflow-y-auto max-h-[300px] scrollbar-thin scrollbar-thumb-slate-700 scrollbar-track-transparent">
                        ${group.files.map(file => `
                            <div class="flex items-center gap-3 p-2 rounded-lg hover:bg-slate-800/50 transition-colors group/file">
                                <div class="flex-1 min-w-0">
                                    <div class="flex items-center gap-2 mb-1">
                                        <span class="text-[10px] font-bold uppercase px-1.5 py-0.5 rounded border ${getCategoryColor(file.category)}">
                                            ${file.category}
                                        </span>
                                        ${file.version ? `
                                            <span class="text-[10px] font-mono text-slate-500 bg-slate-800 px-1.5 py-0.5 rounded">
                                                ${file.version}
                                            </span>
                                        ` : ''}
                                        <span class="text-[10px] font-bold uppercase px-1.5 py-0.5 rounded border ${getFormatColor(file.format)}">
                                            ${file.format}
                                        </span>
                                    </div>
                                    <div class="text-xs text-slate-400 truncate" title="${file.relative_path}">${file.relative_path}</div>
                                </div>
                                
                                <div class="flex items-center gap-3">
                                    <span class="text-xs font-mono text-slate-500">${formatBytes(file.size)}</span>
                                    <a href="/files/${encodeURIComponent(file.relative_path)}" download 
                                       class="p-1.5 text-slate-400 hover:text-white hover:bg-indigo-600 rounded-md transition-colors"
                                       title="Download">
                                        ${Icons.Download}
                                    </a>
                                </div>
                            </div>
                        `).join('')}
                    </div>
                </div>
            `).join('')}
        </div>
    `;
}

const ConnectionModal = () => {
    if (!showConnectionModal) return '';
    
    let hosts: string[] = [];
    if (serverInfo && serverInfo.ips.length > 0) {
        hosts = serverInfo.ips.map(ip => `http://${ip}:${serverInfo!.port}`);
    } else {
        hosts = [window.location.origin];
    }
    
    const renderHosts = (path: string) => hosts.map(h => `
        <div class="bg-slate-950 p-2 rounded border border-slate-800 font-mono text-xs text-slate-300 select-all mb-1">
            ${h}${path}
        </div>
    `).join('');

    const Chevron = `<svg class="w-4 h-4 text-slate-500 transition-transform group-open:rotate-180" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="6 9 12 15 18 9"></polyline></svg>`;
    
    return `
        <div class="fixed inset-0 z-50 flex items-center justify-center p-4">
            <div class="absolute inset-0 bg-black/60 backdrop-blur-sm" onclick="window.toggleModal()"></div>
            <div class="relative bg-slate-900 rounded-2xl border border-slate-700 w-full max-w-lg shadow-2xl overflow-hidden animate-fade-in max-h-[90vh] overflow-y-auto">
                <div class="px-6 py-4 border-b border-slate-800 flex items-center justify-between sticky top-0 bg-slate-900 z-10">
                    <h3 class="text-lg font-bold text-white">Connect to Switch</h3>
                    <button onclick="window.toggleModal()" class="text-slate-400 hover:text-white transition-colors">
                        ${Icons.Close}
                    </button>
                </div>
                <div class="p-6 space-y-4">
                    
                    <!-- DBI -->
                    <details class="group bg-slate-800/30 rounded-lg border border-slate-800 overflow-hidden" open>
                        <summary class="flex items-center justify-between p-4 cursor-pointer hover:bg-slate-800/50 transition-colors select-none list-none">
                            <div class="flex items-center gap-2">
                                <h4 class="font-semibold text-slate-200">DBI Installer</h4>
                                <span class="text-[10px] uppercase font-bold px-1.5 py-0.5 rounded bg-blue-500/20 text-blue-300 border border-blue-500/30">Recommended</span>
                            </div>
                            ${Chevron}
                        </summary>
                        <div class="px-4 pb-4 pt-0 border-t border-transparent group-open:border-slate-800/50">
                            <p class="text-sm text-slate-400 mb-3 mt-2">Use "Install from HTTP" option in DBI on your Switch:</p>
                            ${renderHosts('/dbi')}
                        </div>
                    </details>

                    <!-- Tinfoil -->
                    <details class="group bg-slate-800/30 rounded-lg border border-slate-800 overflow-hidden">
                        <summary class="flex items-center justify-between p-4 cursor-pointer hover:bg-slate-800/50 transition-colors select-none list-none">
                            <div class="flex items-center gap-2">
                                <h4 class="font-semibold text-slate-200">Tinfoil / TinWoo</h4>
                                <span class="text-[10px] uppercase font-bold px-1.5 py-0.5 rounded bg-purple-500/20 text-purple-300 border border-purple-500/30">Alternative</span>
                            </div>
                            ${Chevron}
                        </summary>
                        <div class="px-4 pb-4 pt-0 border-t border-transparent group-open:border-slate-800/50">
                            <p class="text-sm text-slate-400 mb-3 mt-2">Add a new "File" source in File Browser:</p>
                            ${renderHosts('/tinfoil')}
                            
                            <p class="text-xs text-slate-500 mt-3 mb-1 font-medium">For TinWoo specifically:</p>
                             ${renderHosts('/tinwoo')}
                            <div class="mt-2 text-xs text-slate-500">
                                Protocol: HTTP (or Default)
                            </div>
                        </div>
                    </details>

                    <!-- WebDAV -->
                    <details class="group bg-slate-800/30 rounded-lg border border-slate-800 overflow-hidden">
                        <summary class="flex items-center justify-between p-4 cursor-pointer hover:bg-slate-800/50 transition-colors select-none list-none">
                            <div class="flex items-center gap-2">
                                <h4 class="font-semibold text-slate-200">WebDAV</h4>
                                ${serverInfo?.webdav_enabled ? 
                                    `<span class="text-[10px] uppercase font-bold px-1.5 py-0.5 rounded bg-orange-500/20 text-orange-300 border border-orange-500/30">Files</span>` : 
                                    `<span class="text-[10px] uppercase font-bold px-1.5 py-0.5 rounded bg-red-500/20 text-red-300 border border-red-500/30">Disabled</span>`
                                }
                            </div>
                            ${Chevron}
                        </summary>
                        <div class="px-4 pb-4 pt-0 border-t border-transparent group-open:border-slate-800/50">
                            ${serverInfo?.webdav_enabled ? `
                                <p class="text-sm text-slate-400 mb-3 mt-2">Mount as Network Drive or use in Switch file managers:</p>
                                ${renderHosts('/dav')}
                                <div class="mt-3 flex items-center gap-4 text-xs text-slate-400 bg-slate-950/50 p-2 rounded border border-slate-800">
                                    <div class="flex items-center gap-1">
                                        <span class="text-slate-500">Auth:</span>
                                        ${serverInfo.webdav_auth ? '<span class="text-amber-400 font-medium">Required</span>' : '<span class="text-emerald-400 font-medium">None</span>'}
                                    </div>
                                    <div class="flex items-center gap-1">
                                        <span class="text-slate-500">Path:</span>
                                        <span class="font-mono text-slate-300">/dav</span>
                                    </div>
                                </div>
                            ` : `
                                <div class="mt-2 text-sm text-slate-500 bg-slate-950/50 p-3 rounded border border-slate-800 flex items-center gap-2">
                                    <svg class="w-4 h-4 text-slate-600" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z"/></svg>
                                    WebDAV access is disabled in configuration.
                                </div>
                            `}
                        </div>
                    </details>

                </div>
                <div class="px-6 py-4 bg-slate-800/50 border-t border-slate-800 text-center">
                    <button onclick="window.toggleModal()" class="text-sm text-slate-400 hover:text-white transition-colors">Close</button>
                </div>
            </div>
        </div>
    `;
}

// --- Render ---
function render() {
    app.innerHTML = `
        ${Navbar()}
        <main class="container mx-auto px-4 py-8 flex-1">
            ${StatsBar()}
            ${ActiveDownloads()}
            ${GameList()}
        </main>
        ${ConnectionModal()}
    `;
}

// --- Global Handlers ---
declare global {
    interface Window {
        toggleModal: () => void;
    }
}

window.toggleModal = () => {
    showConnectionModal = !showConnectionModal;
    render();
};

// --- Init ---
fetchInfo();
fetchGames();
setupSSE();
render();