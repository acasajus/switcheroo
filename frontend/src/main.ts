import './style.css'

// --- Types ---
interface Game {
  name: string;
  path: string;
  relative_path: string;
  size: number;
  format: string;
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
    status: "scanning" | "complete";
    count: number;
}

interface DownloadUpdate {
    type: "downloads";
    data: Record<string, Download>;
}

type SSEMessage = ScanStatus | DownloadUpdate;

// --- State ---
let games: Game[] = [];
let activeDownloads: Record<string, Download> = {};
let scanStatus: ScanStatus | null = null;
let showConnectionModal = false;

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
        case 'nsp': return 'bg-red-500/20 text-red-400 border-red-500/30';
        case 'nsz': return 'bg-orange-500/20 text-orange-400 border-orange-500/30';
        case 'xci': return 'bg-blue-500/20 text-blue-400 border-blue-500/30';
        case 'xcz': return 'bg-cyan-500/20 text-cyan-400 border-cyan-500/30';
        default: return 'bg-slate-700 text-slate-300';
    }
}

// --- API ---
async function fetchGames() {
  try {
    const response = await fetch('/api/games');
    if (!response.ok) throw new Error('Network response was not ok');
    games = await response.json();
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
    Download: `<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><polyline points="7 10 12 15 17 10"/><line x1="12" y1="15" x2="12" y2="3"/></svg>`,
    Switch: `<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="2" y="6" width="20" height="12" rx="2"/><path d="M6 12h.01M18 12h.01"/></svg>`,
    Wifi: `<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M5 12.55a11 11 0 0 1 14.08 0"/><path d="M1.42 9a16 16 0 0 1 21.16 0"/><path d="M8.53 16.11a6 6 0 0 1 6.95 0"/><line x1="12" y1="20" x2="12.01" y2="20"/></svg>`,
    Close: `<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>`,
    Refresh: `<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8"/><path d="M21 3v5h-5"/><path d="M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16"/><path d="M8 16H3v5"/></svg>`
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
                ${scanStatus && scanStatus.status === 'scanning' ? `
                    <div class="hidden md:flex items-center gap-2 text-xs text-blue-400 bg-blue-500/10 px-3 py-1.5 rounded-full border border-blue-500/20 animate-pulse">
                        <span class="relative flex h-2 w-2">
                          <span class="animate-ping absolute inline-flex h-full w-full rounded-full bg-blue-400 opacity-75"></span>
                          <span class="relative inline-flex rounded-full h-2 w-2 bg-blue-500"></span>
                        </span>
                        Indexing: ${scanStatus.count}
                    </div>
                ` : ''}
                
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
            <div class="text-2xl font-bold text-white">${games.length}</div>
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

const GameGrid = () => {
    if (games.length === 0) {
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
        <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-4">
            ${games.map(game => `
                <div class="group bg-slate-900 rounded-xl border border-slate-800 hover:border-indigo-500/50 transition-all hover:shadow-lg hover:shadow-indigo-500/10 overflow-hidden flex flex-col">
                    <div class="p-4 flex-1">
                        <div class="flex justify-between items-start mb-3">
                            <div class="p-2 bg-slate-800 rounded-lg text-slate-400 group-hover:text-indigo-400 transition-colors">
                                ${Icons.Switch}
                            </div>
                            <span class="text-[10px] font-bold uppercase tracking-wider px-2 py-1 rounded border ${getFormatColor(game.format)}">
                                ${game.format}
                            </span>
                        </div>
                        <h3 class="font-bold text-slate-200 leading-tight mb-1 line-clamp-2" title="${game.name}">${game.name}</h3>
                        <div class="text-xs text-slate-500 truncate mb-4" title="${game.relative_path}">${game.relative_path}</div>
                    </div>
                    <div class="px-4 py-3 bg-slate-800/50 border-t border-slate-800 flex items-center justify-between mt-auto">
                        <span class="text-xs font-mono text-slate-400">${formatBytes(game.size)}</span>
                        <a href="/files/${encodeURIComponent(game.relative_path)}" download 
                           class="flex items-center gap-1.5 px-3 py-1.5 bg-indigo-600 hover:bg-indigo-500 text-white text-xs font-medium rounded-lg transition-colors shadow-lg shadow-indigo-600/20">
                            ${Icons.Download} Download
                        </a>
                    </div>
                </div>
            `).join('')}
        </div>
    `;
}

const ConnectionModal = () => {
    if (!showConnectionModal) return '';
    
    const host = window.location.origin;
    
    return `
        <div class="fixed inset-0 z-50 flex items-center justify-center p-4">
            <div class="absolute inset-0 bg-black/60 backdrop-blur-sm" onclick="window.toggleModal()"></div>
            <div class="relative bg-slate-900 rounded-2xl border border-slate-700 w-full max-w-lg shadow-2xl overflow-hidden animate-fade-in">
                <div class="px-6 py-4 border-b border-slate-800 flex items-center justify-between">
                    <h3 class="text-lg font-bold text-white">Connect to Switch</h3>
                    <button onclick="window.toggleModal()" class="text-slate-400 hover:text-white transition-colors">
                        ${Icons.Close}
                    </button>
                </div>
                <div class="p-6 space-y-6">
                    <div>
                        <div class="flex items-center justify-between mb-2">
                            <h4 class="font-semibold text-slate-200">Tinfoil / TinWoo</h4>
                            <span class="text-xs px-2 py-0.5 rounded bg-purple-500/20 text-purple-300 border border-purple-500/30">Recommended</span>
                        </div>
                        <p class="text-sm text-slate-400 mb-3">Add a new "File" source in File Browser:</p>
                        <div class="bg-slate-950 p-3 rounded-lg border border-slate-800 font-mono text-sm text-slate-300 select-all mb-2">
                            ${host}/tinfoil
                        </div>
                        <p class="text-xs text-slate-500 mb-1">For TinWoo specifically:</p>
                        <div class="bg-slate-950 p-3 rounded-lg border border-slate-800 font-mono text-sm text-slate-300 select-all">
                            ${host}/tinwoo
                        </div>
                        <div class="mt-2 text-xs text-slate-500">
                            Protocol: HTTP (or Default)
                        </div>
                    </div>
                    
                    <div class="border-t border-slate-800 pt-6">
                        <div class="flex items-center justify-between mb-2">
                            <h4 class="font-semibold text-slate-200">DBI Installer</h4>
                            <span class="text-xs px-2 py-0.5 rounded bg-blue-500/20 text-blue-300 border border-blue-500/30">Classic</span>
                        </div>
                        <p class="text-sm text-slate-400 mb-3">Use "Install from HTTP" option:</p>
                        <div class="bg-slate-950 p-3 rounded-lg border border-slate-800 font-mono text-sm text-slate-300 select-all">
                            ${host}/dbi
                        </div>
                    </div>
                </div>
                <div class="px-6 py-4 bg-slate-800/50 border-t border-slate-800 text-center">
                    <button onclick="window.toggleModal()" class="text-sm text-slate-400 hover:text-white">Close</button>
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
            ${GameGrid()}
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
fetchGames();
setupSSE();
render(); // Initial render