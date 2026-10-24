-- Synthetic OpenCode database: both schemas, a fork, and a reply still streaming.
-- Times are milliseconds; 1792400000000 is 2026-10-19T05:33:20Z.
CREATE TABLE message (id TEXT PRIMARY KEY, session_id TEXT, time_created INTEGER, data TEXT);
INSERT INTO message VALUES
  ('msg_u1', 'ses_legacy', 1792400000000,
   '{"id":"msg_u1","sessionID":"ses_legacy","role":"user","time":{"created":1792400000000}}'),
  ('msg_a1', 'ses_legacy', 1792400010000,
   '{"id":"msg_a1","sessionID":"ses_legacy","role":"assistant","modelID":"claude-sonnet-4-5","providerID":"anthropic","path":{"cwd":"/work/project","root":"/work/project"},"cost":0.5,"tokens":{"input":1200,"output":300,"reasoning":0,"cache":{"read":5000,"write":200}},"time":{"created":1792400010000,"completed":1792400020000}}'),
  ('msg_a2', 'ses_legacy', 1792400060000,
   '{"id":"msg_a2","sessionID":"ses_legacy","role":"assistant","modelID":"claude-sonnet-4-5","tokens":{"input":10,"output":0,"reasoning":0,"cache":{"read":0,"write":0}},"time":{"created":1792400060000}}');

CREATE TABLE session_message (id TEXT PRIMARY KEY, session_id TEXT, type TEXT, data TEXT, time_created INTEGER, seq INTEGER);
INSERT INTO session_message VALUES
  ('sm_p1', 'ses_parent', 'user', '{"time":{"created":1792400030000}}', 1792400030000, 1),
  ('sm_p2', 'ses_parent', 'assistant',
   '{"model":{"id":"vendor-model","providerID":"vendor"},"cost":0.02,"tokens":{"input":800,"output":120,"reasoning":30,"cache":{"read":0,"write":0}},"time":{"created":1792400040000,"completed":1792400045000}}',
   1792400040000, 2),
  ('sm_f1', 'ses_fork', 'user', '{"time":{"created":1792400030000}}', 1792400050000, 1),
  ('sm_f2', 'ses_fork', 'assistant',
   '{"model":{"id":"vendor-model","providerID":"vendor"},"cost":0.02,"tokens":{"input":800,"output":120,"reasoning":30,"cache":{"read":0,"write":0}},"time":{"created":1792400040000,"completed":1792400045000}}',
   1792400050000, 2),
  ('sm_f3', 'ses_fork', 'assistant',
   '{"model":{"id":"vendor-model","providerID":"vendor"},"cost":0.01,"tokens":{"input":100,"output":20,"reasoning":0,"cache":{"read":0,"write":0}},"time":{"created":1792400070000,"completed":1792400075000}}',
   1792400070000, 3);

CREATE TABLE session_v2 (id TEXT PRIMARY KEY, fork_session_id TEXT, fork_boundary TEXT);
INSERT INTO session_v2 VALUES
  ('ses_parent', NULL, NULL),
  ('ses_fork', 'ses_parent', '{"type":"through","messageID":"sm_p2"}');
