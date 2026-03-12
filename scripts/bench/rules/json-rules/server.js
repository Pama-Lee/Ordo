const { Engine } = require('json-rules-engine');
const fastify = require('fastify')({ logger: false });

// ─── L1: Trivial ───
function createL1() {
  const e = new Engine();
  e.addRule({ conditions: { all: [{ fact: 'score', operator: 'greaterThanInclusive', value: 60 }] }, event: { type: 'PASS', params: { code: 'PASS', message: 'passed' } }, priority: 2 });
  e.addRule({ conditions: { all: [{ fact: 'score', operator: 'lessThan', value: 60 }] }, event: { type: 'FAIL', params: { code: 'FAIL', message: 'failed' } }, priority: 1 });
  return e;
}

// ─── L2: Simple ───
function createL2() {
  const e = new Engine();
  e.addRule({ conditions: { all: [{ fact: 'score', operator: 'greaterThanInclusive', value: 90 }] }, event: { type: 'HIGH', params: { code: 'HIGH', message: 'high tier' } }, priority: 4 });
  e.addRule({ conditions: { all: [{ fact: 'score', operator: 'greaterThanInclusive', value: 70 }, { fact: 'score', operator: 'lessThan', value: 90 }] }, event: { type: 'MID', params: { code: 'MID', message: 'mid tier' } }, priority: 3 });
  e.addRule({ conditions: { all: [{ fact: 'score', operator: 'greaterThanInclusive', value: 50 }, { fact: 'score', operator: 'lessThan', value: 70 }] }, event: { type: 'LOW', params: { code: 'LOW', message: 'low tier' } }, priority: 2 });
  e.addRule({ conditions: { all: [{ fact: 'score', operator: 'lessThan', value: 50 }] }, event: { type: 'FAIL', params: { code: 'FAIL', message: 'failed' } }, priority: 1 });
  return e;
}

// ─── L3: Medium (simplified — json-rules-engine lacks arithmetic, we approximate) ───
function createL3() {
  const e = new Engine();
  // Gold + high amount
  e.addRule({ conditions: { all: [{ fact: 'membership', operator: 'equal', value: 'gold' }, { fact: 'amount', operator: 'greaterThanInclusive', value: 10000 }] }, event: { type: 'D', params: { code: 'DOMESTIC', message: 'domestic order', discount_rate: 0.30 } }, priority: 10 });
  e.addRule({ conditions: { all: [{ fact: 'membership', operator: 'equal', value: 'gold' }, { fact: 'amount', operator: 'greaterThanInclusive', value: 5000 }, { fact: 'amount', operator: 'lessThan', value: 10000 }] }, event: { type: 'D', params: { code: 'DOMESTIC', message: 'domestic order', discount_rate: 0.25 } }, priority: 9 });
  e.addRule({ conditions: { all: [{ fact: 'membership', operator: 'equal', value: 'gold' }, { fact: 'amount', operator: 'greaterThanInclusive', value: 1000 }, { fact: 'amount', operator: 'lessThan', value: 5000 }] }, event: { type: 'D', params: { code: 'DOMESTIC', message: 'domestic order', discount_rate: 0.20 } }, priority: 8 });
  e.addRule({ conditions: { all: [{ fact: 'membership', operator: 'equal', value: 'gold' }, { fact: 'amount', operator: 'lessThan', value: 1000 }] }, event: { type: 'D', params: { code: 'DOMESTIC', message: 'domestic order', discount_rate: 0.15 } }, priority: 7 });
  // Silver
  e.addRule({ conditions: { all: [{ fact: 'membership', operator: 'equal', value: 'silver' }, { fact: 'amount', operator: 'greaterThanInclusive', value: 10000 }] }, event: { type: 'D', params: { code: 'DOMESTIC', message: 'domestic order', discount_rate: 0.20 } }, priority: 6 });
  e.addRule({ conditions: { all: [{ fact: 'membership', operator: 'equal', value: 'silver' }, { fact: 'amount', operator: 'greaterThanInclusive', value: 5000 }, { fact: 'amount', operator: 'lessThan', value: 10000 }] }, event: { type: 'D', params: { code: 'DOMESTIC', message: 'domestic order', discount_rate: 0.15 } }, priority: 5 });
  e.addRule({ conditions: { all: [{ fact: 'membership', operator: 'equal', value: 'silver' }, { fact: 'amount', operator: 'lessThan', value: 5000 }] }, event: { type: 'D', params: { code: 'DOMESTIC', message: 'domestic order', discount_rate: 0.10 } }, priority: 4 });
  // Bronze
  e.addRule({ conditions: { all: [{ fact: 'amount', operator: 'greaterThanInclusive', value: 10000 }] }, event: { type: 'D', params: { code: 'DOMESTIC', message: 'domestic order', discount_rate: 0.10 } }, priority: 2 });
  e.addRule({ conditions: { all: [{ fact: 'amount', operator: 'lessThan', value: 10000 }] }, event: { type: 'D', params: { code: 'DOMESTIC', message: 'domestic order', discount_rate: 0.05 } }, priority: 1 });
  return e;
}

// ─── L4: Complex (simplified — use nested conditions to approximate multi-stage) ───
function createL4() {
  const e = new Engine();
  // Critical amount + high risk → BLOCK
  e.addRule({ conditions: { all: [{ fact: 'txn_amount', operator: 'greaterThanInclusive', value: 100000 }, { fact: 'risk_score', operator: 'greaterThanInclusive', value: 80 }, { fact: 'trust_level', operator: 'lessThan', value: 8 }] }, event: { type: 'BLOCK', params: { code: 'BLOCK', message: 'transaction blocked' } }, priority: 20 });
  e.addRule({ conditions: { all: [{ fact: 'txn_amount', operator: 'greaterThanInclusive', value: 100000 }, { fact: 'risk_score', operator: 'greaterThanInclusive', value: 80 }, { fact: 'trust_level', operator: 'greaterThanInclusive', value: 8 }] }, event: { type: 'REVIEW', params: { code: 'REVIEW', message: 'manual review required' } }, priority: 19 });
  e.addRule({ conditions: { all: [{ fact: 'txn_amount', operator: 'greaterThanInclusive', value: 100000 }, { fact: 'risk_score', operator: 'greaterThanInclusive', value: 50 }, { fact: 'risk_score', operator: 'lessThan', value: 80 }] }, event: { type: 'REVIEW', params: { code: 'REVIEW', message: 'manual review required' } }, priority: 18 });
  e.addRule({ conditions: { all: [{ fact: 'txn_amount', operator: 'greaterThanInclusive', value: 100000 }, { fact: 'risk_score', operator: 'lessThan', value: 50 }] }, event: { type: 'FLAG', params: { code: 'FLAG', message: 'flagged for monitoring' } }, priority: 17 });
  // High amount
  e.addRule({ conditions: { all: [{ fact: 'txn_amount', operator: 'greaterThanInclusive', value: 50000 }, { fact: 'txn_amount', operator: 'lessThan', value: 100000 }, { fact: 'risk_score', operator: 'greaterThanInclusive', value: 80 }] }, event: { type: 'REVIEW', params: { code: 'REVIEW', message: 'manual review required' } }, priority: 16 });
  e.addRule({ conditions: { all: [{ fact: 'txn_amount', operator: 'greaterThanInclusive', value: 50000 }, { fact: 'txn_amount', operator: 'lessThan', value: 100000 }, { fact: 'risk_score', operator: 'greaterThanInclusive', value: 50 }, { fact: 'risk_score', operator: 'lessThan', value: 80 }] }, event: { type: 'FLAG', params: { code: 'FLAG', message: 'flagged for monitoring' } }, priority: 15 });
  e.addRule({ conditions: { all: [{ fact: 'txn_amount', operator: 'greaterThanInclusive', value: 50000 }, { fact: 'txn_amount', operator: 'lessThan', value: 100000 }, { fact: 'risk_score', operator: 'lessThan', value: 50 }] }, event: { type: 'MONITOR', params: { code: 'MONITOR', message: 'enhanced monitoring' } }, priority: 14 });
  // Medium amount
  e.addRule({ conditions: { all: [{ fact: 'txn_amount', operator: 'greaterThanInclusive', value: 10000 }, { fact: 'txn_amount', operator: 'lessThan', value: 50000 }, { fact: 'risk_score', operator: 'greaterThanInclusive', value: 80 }] }, event: { type: 'FLAG', params: { code: 'FLAG', message: 'flagged for monitoring' } }, priority: 13 });
  e.addRule({ conditions: { all: [{ fact: 'txn_amount', operator: 'greaterThanInclusive', value: 10000 }, { fact: 'txn_amount', operator: 'lessThan', value: 50000 }, { fact: 'risk_score', operator: 'lessThan', value: 80 }] }, event: { type: 'MONITOR', params: { code: 'MONITOR', message: 'enhanced monitoring' } }, priority: 12 });
  // Low amount
  e.addRule({ conditions: { all: [{ fact: 'txn_amount', operator: 'greaterThanInclusive', value: 1000 }, { fact: 'txn_amount', operator: 'lessThan', value: 10000 }, { fact: 'risk_score', operator: 'greaterThanInclusive', value: 90 }] }, event: { type: 'FLAG', params: { code: 'FLAG', message: 'flagged for monitoring' } }, priority: 11 });
  e.addRule({ conditions: { all: [{ fact: 'txn_amount', operator: 'greaterThanInclusive', value: 1000 }, { fact: 'txn_amount', operator: 'lessThan', value: 10000 }] }, event: { type: 'PASS', params: { code: 'PASS', message: 'approved' } }, priority: 10 });
  // Minimal amount
  e.addRule({ conditions: { all: [{ fact: 'txn_amount', operator: 'lessThan', value: 1000 }, { fact: 'risk_score', operator: 'greaterThanInclusive', value: 95 }] }, event: { type: 'FLAG', params: { code: 'FLAG', message: 'flagged for monitoring' } }, priority: 9 });
  e.addRule({ conditions: { all: [{ fact: 'txn_amount', operator: 'lessThan', value: 1000 }] }, event: { type: 'PASS', params: { code: 'PASS', message: 'approved' } }, priority: 1 });
  return e;
}

const engines = { L1: createL1(), L2: createL2(), L3: createL3(), L4: createL4() };

// ─── Routes ───
fastify.post('/execute/:level', async (req) => {
  const level = req.params.level;
  const engine = engines[level];
  if (!engine) return { code: 'ERROR', message: 'unknown level' };
  const { events } = await engine.run(req.body);
  return events[0] ? events[0].params : { code: 'NONE', message: 'no match' };
});

fastify.get('/health', async () => ({ status: 'ok' }));
fastify.listen({ port: 8080, host: '0.0.0.0' }).then(() => console.log('json-rules-engine on :8080'));
