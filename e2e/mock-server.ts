import http from 'http';

export interface MockState {
  sttScenario: 'success' | 'delayed' | 'fail_429' | 'fail_400' | 'fallback_retry';
  sttDelayMs: number;
  transcriptText: string;
  aiScenario: 'success' | 'delayed' | 'fail_429' | 'fail_400' | 'fallback_retry';
  aiDelayMs: number;
  aiAnswerText: string;
  receivedSttRequests: number;
  receivedAiRequests: number;
}

export const defaultMockState: MockState = {
  sttScenario: 'success',
  sttDelayMs: 0,
  transcriptText: 'Bagaimana cara kerja memory safety di Rust?',
  aiScenario: 'success',
  aiDelayMs: 0,
  aiAnswerText: 'Rust menjamin **memory safety** saat compile-time lewat sistem **ownership**, **borrowing**, dan **lifetimes** tanpa garbage collector.',
  receivedSttRequests: 0,
  receivedAiRequests: 0,
};

let currentState: MockState = { ...defaultMockState };

export function resetMockState(overrides: Partial<MockState> = {}) {
  currentState = { ...defaultMockState, ...overrides };
}

export function getMockState(): MockState {
  return currentState;
}

export function createMockServer(): http.Server {
  const server = http.createServer(async (req, res) => {
    const url = req.url || '';
    const authHeader = req.headers['authorization'] || '';

    if (url.startsWith('/groq') || url.startsWith('/openrouter')) {
      console.log('[MOCK SERVER]', req.method, url);
      req.resume(); // Ensure incoming stream is drained
    }

    // Endpoint kontrol per-test
    if (url === '/__mock_control' && req.method === 'POST') {
      let body = '';
      req.on('data', (chunk) => (body += chunk));
      req.on('end', () => {
        try {
          const parsed = JSON.parse(body);
          currentState = { ...currentState, ...parsed };
          res.writeHead(200, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({ status: 'ok', state: currentState }));
        } catch {
          res.writeHead(400);
          res.end('Invalid JSON');
        }
      });
      return;
    }

    if (url === '/__mock_state' && req.method === 'GET') {
      res.writeHead(200, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify(currentState));
      return;
    }

    // Endpoint STT (Groq Whisper)
    if (url.startsWith('/groq')) {
      currentState.receivedSttRequests++;
      const scenario = currentState.sttScenario;

      if (scenario === 'fail_429') {
        res.writeHead(429, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ error: { message: 'Groq API rate limit reached (HTTP 429)' } }));
        return;
      }

      if (scenario === 'fail_400') {
        res.writeHead(400, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ error: { message: 'Bad request invalid audio payload (HTTP 400)' } }));
        return;
      }

      if (scenario === 'fallback_retry') {
        if (authHeader.includes('mock-groq-key') || authHeader.includes('primary')) {
          res.writeHead(429, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({ error: { message: 'Primary key 429' } }));
          return;
        }
        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ text: currentState.transcriptText }));
        return;
      }

      if (scenario === 'delayed' && currentState.sttDelayMs > 0) {
        await new Promise((r) => setTimeout(r, currentState.sttDelayMs));
      }

      res.writeHead(200, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify({ text: currentState.transcriptText }));
      return;
    }

    // Endpoint AI (OpenRouter Chat Completion)
    if (url.startsWith('/openrouter')) {
      currentState.receivedAiRequests++;
      const scenario = currentState.aiScenario;

      if (scenario === 'fail_429') {
        res.writeHead(429, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ error: { message: 'OpenRouter rate limit reached (HTTP 429)' } }));
        return;
      }

      if (scenario === 'fail_400') {
        res.writeHead(400, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ error: { message: 'Bad request invalid chat format (HTTP 400)' } }));
        return;
      }

      if (scenario === 'fallback_retry') {
        if (authHeader.includes('mock-openrouter-key') || authHeader.includes('primary')) {
          res.writeHead(429, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({ error: { message: 'Primary OpenRouter 429' } }));
          return;
        }
        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(
          JSON.stringify({
            choices: [{ message: { role: 'assistant', content: currentState.aiAnswerText } }],
          })
        );
        return;
      }

      if (scenario === 'delayed' && currentState.aiDelayMs > 0) {
        await new Promise((r) => setTimeout(r, currentState.aiDelayMs));
      }

      res.writeHead(200, { 'Content-Type': 'application/json' });
      res.end(
        JSON.stringify({
          choices: [{ message: { role: 'assistant', content: currentState.aiAnswerText } }],
        })
      );
      return;
    }

    res.writeHead(404);
    res.end('Not Found');
  });

  return server;
}
