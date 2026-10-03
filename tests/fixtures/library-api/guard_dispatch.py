import json
import os
import subprocess

root = '/dev/kakoi-guard/bin/'
for name, expected in [('tool', 7), ('tool2', 8), ('tool (deleted)', 9), ('tool (deleted) (deleted)', 10)]:
    assert subprocess.call([root + name]) == expected, name
    assert subprocess.call([root + name, 'blocked']) == 126, name
    assert subprocess.call([root + name, 'wrong']) == expected, name
identities = {(os.stat(root + name).st_dev, os.stat(root + name).st_ino)
              for name in ['tool', 'tool2', 'tool (deleted)', 'tool (deleted) (deleted)']}
assert len(identities) == 4

# A copy outside the listed table cannot borrow another copy's rules by its name.
data = open(root + 'tool', 'rb').read()
magic_at = data.rindex(b'\0KAKOI-ROLE-V1\0')
for content in [data, data[:-1] + b'\xff', data[:magic_at] + b'X' + data[magic_at+1:]]:
    fd = os.memfd_create('copy', 0)
    os.write(fd, content)
    os.lseek(fd, 0, 0)
    args = ['/usr/bin/bwrap', '--ro-bind', '/', '/', '--proc', '/proc', '--tmpfs', '/tmp', '--perms', '0555',
            '--ro-bind-data', str(fd), '/tmp/guard-copy', '--', '/tmp/guard-copy']
    assert subprocess.call(args, pass_fds=(fd,)) == 126
    os.close(fd)

# A missing/corrupt table never invokes the consumer application.
for table in [None, b'', b'{invalid', json.dumps({'entries': []}).encode()]:
    image = os.memfd_create('guard', 0)
    os.write(image, data)
    os.lseek(image, 0, 0)
    args = ['/usr/bin/bwrap', '--ro-bind', '/', '/', '--proc', '/proc',
            '--dev', '/dev', '--dir', '/dev/kakoi-guard/bin', '--perms', '0555',
            '--ro-bind-data', str(image), root + 'tool']
    fds = [image]
    if table is not None:
        fd = os.memfd_create('table', 0)
        os.write(fd, table)
        os.lseek(fd, 0, 0)
        args += ['--ro-bind-data', str(fd), '/dev/kakoi-guard/table']
        fds.append(fd)
    args += ['--', root + 'tool']
    assert subprocess.call(args, pass_fds=fds) == 126
    for fd in fds:
        os.close(fd)
print('dispatch passed')
