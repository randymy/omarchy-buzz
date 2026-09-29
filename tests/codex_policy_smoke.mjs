// Dependency-free policy checks; not an adapter build or subscription acceptance.
// Usage: node tests/codex_policy_smoke.mjs /path/to/patched/codex-acp
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {stripTypeScriptTypes} from 'node:module';
import {resolve} from 'node:path';

if (process.argv.length !== 3) throw new Error('Expected patched adapter source directory');
const source = readFileSync(resolve(process.argv[2], 'src/SubscriptionPolicy.ts'), 'utf8');
const javascript = stripTypeScriptTypes(source, {mode: 'strip'});
const {SubscriptionPolicy, validSubscriptionConfig, validateSubscriptionStartup,
  subscriptionChildEnv} = await import(`data:text/javascript;base64,${Buffer.from(javascript).toString('base64')}`);

const clean = {forced_login_method: 'chatgpt', model_provider: 'openai'};
assert.equal(validSubscriptionConfig(clean), true);
for (const config of [null, [], {}, {...clean, forced_login_method: 'api'},
  {...clean, model_provider: 'gateway'}, {...clean, model_providers: {openai: {}}},
  ...['openai_base_url', 'chatgpt_base_url', 'model_catalog_url', 'experimental_bearer_token']
    .map(key => ({...clean, [key]: 'synthetic'}))]) {
  assert.equal(validSubscriptionConfig(config), false);
}
validateSubscriptionStartup({CODEX_HOME: '/synthetic/profile'});
for (const env of [{}, {CODEX_HOME: 'relative'},
  ...['OPENAI_API_KEY', 'CODEX_API_KEY', 'MODEL_PROVIDER', 'CODEX_CONFIG', 'CODEX_PATH',
    'DEFAULT_AUTH_REQUEST', 'NODE_OPTIONS', 'HTTPS_PROXY']
    .map(key => ({CODEX_HOME: '/synthetic/profile', [key]: 'synthetic'}))]) {
  assert.throws(() => validateSubscriptionStartup(env), /^Error: subscription_policy_denied$/);
}
assert.deepEqual(subscriptionChildEnv({CODEX_HOME: '/synthetic/profile',
  UNRELATED_SECRET: 'synthetic', OPENAI_API_KEY: 'synthetic', https_proxy: 'synthetic'}),
  {CODEX_HOME: '/synthetic/profile'});

let config = clean;
let account = {type: 'chatgpt'};
let calls = 0;
let fail = false;
const policy = new SubscriptionPolicy({
  async configRead(params) {
    assert.deepEqual(params, {cwd: '/synthetic/workspace', includeLayers: false});
    calls++;
    if (fail) throw new Error('synthetic sensitive failure');
    return {config};
  },
  async accountRead(params) {
    assert.deepEqual(params, {refreshToken: false});
    return {account};
  },
});
const denied = /^Error: subscription_policy_denied$/;
await assert.rejects(policy.checkPrompt('unknown'), denied);
await assert.rejects(policy.checkWorkspace('relative'), denied);
await policy.checkWorkspace('/synthetic/workspace');
policy.remember('fresh', '/synthetic/workspace');
await policy.checkPrompt('fresh');
assert.equal(calls, 2);
for (const changed of [null, {type: 'apiKey'}, {type: 'unexpected'}]) {
  account = changed;
  await assert.rejects(policy.checkPrompt('fresh'), denied);
}
account = {type: 'chatgpt'};
config = {...clean, model_provider: 'gateway'};
await assert.rejects(policy.checkPrompt('fresh'), denied);
config = clean;
fail = true;
await assert.rejects(policy.checkPrompt('fresh'), denied);
console.log('Policy module smoke checks passed; adapter build and native acceptance remain unverified.');
