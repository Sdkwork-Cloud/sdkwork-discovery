import io, re

def read(p):
    return io.open(p, 'r', encoding='utf-8').read().replace('\r\n', '\n')

def write(p, c):
    io.open(p, 'w', encoding='utf-8', newline='\n').write(c)

def brace_block_end(lines, i):
    j = i
    brace = 0
    found = False
    while j < len(lines):
        if not found:
            if '{' in lines[j]:
                found = True
                brace = lines[j].count('{') - lines[j].count('}')
            j += 1
            continue
        brace += lines[j].count('{') - lines[j].count('}')
        if brace <= 0:
            return j
        j += 1
    return len(lines) - 1

p = 'services/sdkwork-discovery-service-host/src/bootstrap.rs'
c = read(p)
lines = c.split('\n')
out = []
i = 0
removed = 0
while i < len(lines):
    stripped = lines[i].strip()
    is_import = bool(re.match(r'use .*[sS]qlite.*;', stripped))
    is_variant = bool(re.match(r'Sqlite \{', stripped))
    is_arm = bool(re.match(r'(DiscoveryRuntimeStorage|StorageProvider)::Sqlite', stripped))
    is_field = bool(re.search(r'SqliteDiscoveryStore', stripped))
    if is_import:
        i += 1
        removed += 1
        continue
    if is_variant:
        end = brace_block_end(lines, i)
        i = end + 1
        removed += 1
        continue
    if is_arm:
        # block arm or expression arm; find its end by brace/paren balance
        j = i
        brace = 0
        paren = 0
        found = False
        while j < len(lines):
            if not found:
                if '{' in lines[j] or '(' in lines[j]:
                    found = True
                    brace = lines[j].count('{') - lines[j].count('}')
                    paren = lines[j].count('(') - lines[j].count(')')
                j += 1
                continue
            brace += lines[j].count('{') - lines[j].count('}')
            paren += lines[j].count('(') - lines[j].count(')')
            if brace <= 0 and paren <= 0:
                break
            j += 1
        end = j
        if lines[j].strip() == ',':
            end = j + 1
        elif lines[j].rstrip().endswith('},') or lines[j].rstrip().endswith('),'):
            end = j + 1
        i = end + 1
        removed += 1
        continue
    if is_field:
        i += 1
        removed += 1
        continue
    out.append(lines[i])
    i += 1
c = '\n'.join(out)
rem = [ln for ln in c.split('\n') if re.search(r'[sS]qlite', ln)]
print('removed:', removed, '| remaining:', len(rem))
for r in rem[:8]:
    print('  ', r.strip()[:85])
print('balance:', c.count('{') - c.count('}'))
write(p, c)
print('bootstrap.rs WROTE (lenient)')
