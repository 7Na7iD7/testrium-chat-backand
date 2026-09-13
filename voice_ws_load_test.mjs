import WebSocket from 'ws';

const WS_BASE_URL = process.env.VOICE_WS_URL || 'ws://localhost:8080/ws';
const CONNECTION_COUNT = Number(process.env.LOAD_TEST_CONNECTIONS || 50);
const TEST_DURATION_MS = Number(process.env.LOAD_TEST_DURATION_MS || 30000);
const EVENTS_PER_SECOND = Number(process.env.LOAD_TEST_EVENTS_PER_SEC || 5);
const ROOM_ID = process.env.LOAD_TEST_ROOM_ID || '00000000-0000-0000-0000-000000000000';

const tokens = (process.env.LOAD_TEST_TOKENS || '')
  .split(',')
  .map((t) => t.trim())
  .filter(Boolean);

if (tokens.length === 0) {
  console.error('LOAD_TEST_TOKENS خالی است. یک لیست توکن معتبر با کاما جدا شده بده.');
  process.exit(1);
}

const stats = {
  connected: 0,
  failed: 0,
  disconnected: 0,
  sent: 0,
  received: 0,
  errors: 0,
};

function openConnection(index) {
  const token = tokens[index % tokens.length];
  const url = `${WS_BASE_URL}?token=${encodeURIComponent(token)}`;
  const socket = new WebSocket(url);
  let sendTimer = null;

  socket.on('open', () => {
    stats.connected += 1;
    sendTimer = setInterval(() => {
      const payload = JSON.stringify({
        type: 'voice_speaking_state_changed',
        room_id: ROOM_ID,
        speaking: Math.random() > 0.5,
      });
      try {
        socket.send(payload);
        stats.sent += 1;
      } catch (_) {
        stats.errors += 1;
      }
    }, 1000 / EVENTS_PER_SECOND);
  });

  socket.on('message', () => {
    stats.received += 1;
  });

  socket.on('error', () => {
    stats.errors += 1;
  });

  socket.on('close', () => {
    stats.disconnected += 1;
    if (sendTimer) clearInterval(sendTimer);
  });

  return socket;
}

function printStats() {
  console.log(
    `connected=${stats.connected} disconnected=${stats.disconnected} sent=${stats.sent} received=${stats.received} errors=${stats.errors} rssMB=${(process.memoryUsage().rss / 1024 / 1024).toFixed(1)}`,
  );
}

async function run() {
  console.log(`شروع تست بار با ${CONNECTION_COUNT} اتصال هم‌زمان به ${WS_BASE_URL}`);

  const sockets = [];
  for (let i = 0; i < CONNECTION_COUNT; i += 1) {
    sockets.push(openConnection(i));
    await new Promise((resolve) => setTimeout(resolve, 20));
  }

  const statsInterval = setInterval(printStats, 2000);

  await new Promise((resolve) => setTimeout(resolve, TEST_DURATION_MS));

  clearInterval(statsInterval);
  printStats();

  for (const socket of sockets) {
    socket.close();
  }

  await new Promise((resolve) => setTimeout(resolve, 1000));
  process.exit(0);
}

run();
