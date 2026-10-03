use super::{consumer, fake_host::FakeHost};

// @kotowari[REQ-098, EX-214, REQ-library-201, REQ-library-402]
#[test]
fn terminal_ctrl_c_allows_the_application_to_continue_in_every_network_mode() {
    let host = FakeHost::new("", &[]);
    host.run(&format!(r#"
import select, signal, time
for mode in ['filtered','host','none']:
 master,slave=os.openpty()
 def terminal():
  os.setsid()
  import fcntl,termios
  fcntl.ioctl(0,termios.TIOCSCTTY,0)
 p=subprocess.Popen([{consumer:?},'--self-test-terminal-continue'],cwd=os.environ['WORKSPACE'],
  stdin=slave,stdout=slave,stderr=slave,preexec_fn=terminal,
  env={{'PATH':os.environ['BIN']+':/usr/sbin:/usr/bin:/bin','HOME':os.environ['HOME_DIR'],'KAKOI_TEST_SIGNAL_NETWORK':mode}})
 os.close(slave)
 data=b''
 try:
  deadline=time.monotonic()+30
  while b'app-ready' not in data:
   assert time.monotonic()<deadline,data
   if select.select([master],[],[],0.1)[0]: data+=os.read(master,65536)
  os.write(master,b'\x03')
  while p.poll() is None:
   assert time.monotonic()<deadline,data
   if select.select([master],[],[],0.1)[0]:
    try: data+=os.read(master,65536)
    except OSError: break
  p.wait(timeout=10)
  while select.select([master],[],[],0)[0]:
   try: data+=os.read(master,65536)
   except OSError: break
  assert p.returncode==0,(mode,data)
  assert b'continued caught=1' in data,(mode,data)
  assert b'Completed' in data,(mode,data)
  assert not pastas(),pastas()
 finally:
  try: os.killpg(p.pid,signal.SIGKILL)
  except ProcessLookupError: pass
  p.wait();os.close(master)
"#,consumer=consumer().to_str().unwrap()));
}

// @kotowari[REQ-library-201, REQ-library-202, REQ-148]
#[test]
fn filtered_terminal_interrupt_preserves_the_worker_and_confirms_cleanup() {
    let host = FakeHost::new("", &[]);
    host.run(&format!(r#"
import signal
for trigger in ['interrupt','terminate','stop']:
 env={{'PATH':os.environ['BIN']+':/usr/sbin:/usr/bin:/bin','HOME':os.environ['HOME_DIR'],'KAKOI_TEST_SIGNAL_NETWORK':'filtered'}}
 if trigger=='terminate': env['KAKOI_TEST_SIGNAL_TERM']='1'
 if trigger=='stop': env['KAKOI_TEST_SIGNAL_CONTROL']='1'
 p=subprocess.Popen([{consumer:?},'--self-test-terminal-signal'],cwd=os.environ['WORKSPACE'],
  start_new_session=True,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,bufsize=0,env=env)
 try:
  assert line(p.stdout).strip()=='terminal-ready'
  if trigger=='interrupt': os.killpg(p.pid,signal.SIGINT)
  elif trigger=='terminate':
   workers=open('/proc/'+str(p.pid)+'/task/'+str(p.pid)+'/children').read().split()
   assert len(workers)==1,workers
   os.kill(int(workers[0]),signal.SIGTERM)
  out,err=p.communicate(b'stop\n' if trigger=='stop' else None,timeout=30)
  assert p.returncode==0,(trigger,out,err)
  assert not pastas(),pastas()
 finally:
  try: os.killpg(p.pid,signal.SIGKILL)
  except ProcessLookupError: pass
  p.wait()
"#,consumer=consumer().to_str().unwrap()));
}

// @kotowari[REQ-library-204]
#[test]
fn filtered_publications_can_be_reused_after_real_traffic_and_confirmed_shutdown() {
    let host = FakeHost::new("", &[]);
    host.run(&format!(
        r#"
for _ in range(2):
 process=subprocess.Popen([{consumer:?},'--self-test-filtered-owner'],cwd=os.environ['WORKSPACE'],
  stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,bufsize=0,
  env={{'PATH':os.environ['BIN']+':/usr/sbin:/usr/bin:/bin','HOME':os.environ['HOME_DIR']}})
 ready=line(process.stdout).strip()
 assert ready=='ready',(ready,process.stderr.read())
 assert exchange('127.0.0.1',23451,'tcp',PERMITTED)=='published'
 process.stdin.write(b'stop\n');process.stdin.flush()
 out,err=process.communicate(timeout=30)
 assert process.returncode==0,(out,err)
 assert not pastas(),pastas()
"#,
        consumer = consumer().to_str().unwrap()
    ));
}

// @kotowari[REQ-library-206, REQ-library-402, EX-library-211, EX-library-212]
#[test]
fn nested_api_inside_filtered_retains_outer_restrictions_and_guards() {
    let host = FakeHost::new("", &[]);
    host.run(&format!(r#"
workspace=os.environ['WORKSPACE']
open(workspace+'/secret','w').write('secret')
os.mkdir(workspace+'/tools');os.mkdir(workspace+'/failing')
open(workspace+'/tools/tool','w').write('#!/bin/sh\nprintf unguarded\n')
os.chmod(workspace+'/tools/tool',0o755)
open(workspace+'/failing/bwrap','w').write('#!/bin/sh\nif [ "$1" = --help ]; then exec /usr/bin/bwrap --help; fi\nexit 1\n')
os.chmod(workspace+'/failing/bwrap',0o755)
process=subprocess.run(['/usr/bin/unshare','--user','--map-user=1000','--map-group=1000',{consumer:?},'--self-test-api-nested'],
 cwd=workspace,stdin=subprocess.DEVNULL,stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=60,
 env={{'PATH':workspace+'/tools:'+os.environ['BIN']+':/usr/sbin:/usr/bin:/bin','HOME':os.environ['HOME_DIR'],'KAKOI_TEST_FILTERED_NESTED':'1'}})
assert process.returncode==0,(process.stdout,process.stderr)
assert not pastas(),pastas()
"#,consumer=consumer().to_str().unwrap()));
}

// @kotowari[REQ-library-201, REQ-library-202, REQ-library-203, REQ-library-204, REQ-library-301, REQ-library-409, REQ-148, EX-library-202, EX-library-204, EX-library-205, EX-library-207, EX-library-208, EX-library-301, EX-library-415]
#[test]
fn filtered_real_traffic_is_blocked_before_termination_and_processes_are_reaped() {
    let host = FakeHost::new("", &[]);
    host.run(&format!(
        r#"
serve('127.0.0.1', 23450, 'tcp', 'permitted')
process = subprocess.Popen([{consumer:?}, '--self-test-filtered'],
    cwd=os.environ['WORKSPACE'], stdout=subprocess.PIPE, stderr=subprocess.PIPE,
    env={{'PATH': os.environ['BIN'] + ':/usr/sbin:/usr/bin:/bin', 'HOME': os.environ['HOME_DIR']}})
out, err = process.communicate(timeout=60)
assert process.returncode == 0, (out, err)
assert b'permitted' in out, (out, err)
assert exchange('127.0.0.1', 23451, 'tcp', REFUSED).startswith('failed')
assert not pastas(), pastas()
print(out.decode())
"#,
        consumer = consumer().to_str().unwrap()
    ));
}

// @kotowari[REQ-library-202, REQ-library-203, EX-library-203, EX-library-205]
#[test]
fn filtered_stop_and_drop_reap_descendants_even_with_unread_output_and_held_pipes() {
    let host = FakeHost::new("", &[]);
    host.run(&format!(r#"
process=subprocess.run([{consumer:?},'--self-test-lifetime'],cwd=os.environ['WORKSPACE'],
 stdin=subprocess.DEVNULL,stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=60,
 env={{'PATH':os.environ['BIN']+':/usr/sbin:/usr/bin:/bin','HOME':os.environ['HOME_DIR'],'KAKOI_TEST_FILTERED_SCENARIOS':'1'}})
assert process.returncode==0,(process.stdout,process.stderr)
assert not pastas(),pastas()
"#,consumer=consumer().to_str().unwrap()));
}

// @kotowari[REQ-library-203, EX-library-206]
#[test]
fn filtered_owner_death_closes_traffic_and_removes_helpers_without_drop() {
    let host = FakeHost::new("", &[]);
    host.run(&format!(r#"
import time
before=set(os.listdir('/proc'))
process = subprocess.Popen([{consumer:?}, '--self-test-filtered-owner'],
    cwd=os.environ['WORKSPACE'], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, bufsize=0,
    env={{'PATH': os.environ['BIN'] + ':/usr/sbin:/usr/bin:/bin', 'HOME': os.environ['HOME_DIR']}})
try:
 ready=line(process.stdout).strip()
except AssertionError:
 process.kill();out,err=process.communicate(timeout=10);raise AssertionError((out,err))
assert ready == 'ready', (ready,process.stderr.read() if process.poll() is not None else b'')
assert exchange('127.0.0.1',23451,'tcp',PERMITTED) == 'published'
owned=set(os.listdir('/proc'))-before
process.kill(); process.wait(timeout=20)
deadline=time.monotonic()+30
while owned.intersection(os.listdir('/proc')):
 assert time.monotonic()<deadline, owned.intersection(os.listdir('/proc'))
 time.sleep(0.02)
assert exchange('127.0.0.1',23451,'tcp',REFUSED).startswith('failed')
assert not pastas(), pastas()
"#,consumer=consumer().to_str().unwrap()));
}

// @kotowari[REQ-library-201, EX-library-201, REQ-148, REQ-library-302, REQ-068]
#[test]
fn filtered_recovers_forwarders_while_the_caller_is_not_waiting() {
    let host = FakeHost::new("", &[]);
    host.run(&format!(r#"
import time
process = subprocess.Popen([{consumer:?}, '--self-test-filtered-owner'],
    cwd=os.environ['WORKSPACE'], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, bufsize=0,
    env={{'PATH': os.environ['BIN'] + ':/usr/sbin:/usr/bin:/bin', 'HOME': os.environ['HOME_DIR'], 'KAKOI_TEST_RECOVERY_EVENTS':'1'}})
try:
 ready=line(process.stdout).strip()
except AssertionError:
 process.kill();out,err=process.communicate(timeout=10);raise AssertionError((out,err))
assert ready == 'ready', (ready,process.stderr.read() if process.poll() is not None else b'')
assert exchange('127.0.0.1',23451,'tcp',PERMITTED) == 'published'
[old]=[pid for pid,words in pastas() if b'--map-host-loopback' in words]
os.kill(old,9)
deadline=time.monotonic()+30
while True:
 if old not in [pid for pid,_ in pastas()] and exchange('127.0.0.1',23451,'tcp',1)=='published': break
 assert time.monotonic()<deadline, pastas()
 time.sleep(0.02)
process.stdin.write(b'stop\n');process.stdin.flush()
out,err=process.communicate(timeout=30)
assert process.returncode==0,(out,err)
assert not pastas(),pastas()
"#,consumer=consumer().to_str().unwrap()));
}

// @kotowari[REQ-library-301, EX-library-302]
#[test]
fn filtered_cutoff_failure_is_not_confused_with_process_cleanup() {
    let host = FakeHost::new("", &[]);
    let trigger = host.write("stall-trigger", "");
    std::fs::remove_file(&trigger).unwrap();
    host.command("nft",&format!("#!/bin/sh\nif [ -e {:?} ] && [ \"$1\" = -f ]; then exec /bin/sleep 10; fi\nexec /usr/sbin/nft \"$@\"\n",trigger));
    host.run(&format!(r#"
process=subprocess.Popen([{consumer:?},'--self-test-filtered-owner'],
 cwd=os.environ['WORKSPACE'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,bufsize=0,
 env={{'PATH':os.environ['BIN']+':/usr/sbin:/usr/bin:/bin','HOME':os.environ['HOME_DIR'],'KAKOI_TEST_CLOSURE_FAILURE':'1'}})
assert line(process.stdout).strip()=='ready'
open({trigger:?},'w').close()
process.stdin.write(b'stop\n');process.stdin.flush()
out,err=process.communicate(timeout=30)
assert process.returncode==0,(out,err)
assert exchange('127.0.0.1',23451,'tcp',REFUSED).startswith('failed')
"#,consumer=consumer().to_str().unwrap(),trigger=trigger.to_str().unwrap()));
}
