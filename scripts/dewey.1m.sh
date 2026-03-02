#!/bin/bash

# <xbar.title>Dewey Tasks</xbar.title>
# <xbar.version>v1.0</xbar.version>
# <xbar.author>keyfer</xbar.author>
# <xbar.desc>Show tasks from Dewey in the menu bar</xbar.desc>
# <xbar.dependencies>dewey</xbar.dependencies>

export PATH="$HOME/.cargo/bin:$PATH"

# Get tasks as JSON
TASKS_JSON=$(dewey list all --format json 2>/dev/null)

if [ $? -ne 0 ] || [ -z "$TASKS_JSON" ] || [ "$TASKS_JSON" = "[]" ]; then
    echo "✓"
    echo "---"
    echo "All done!"
    echo "---"
    echo "Add Task... | bash=$HOME/.cargo/bin/dewey param1=tui terminal=true"
    exit 0
fi

# Count tasks
TOTAL=$(echo "$TASKS_JSON" | /usr/bin/python3 -c "import sys,json; print(len(json.load(sys.stdin)))" 2>/dev/null || echo "0")

# Get today's date
TODAY=$(/bin/date +%Y-%m-%d)

# Count overdue and today tasks
read OVERDUE TODAY_COUNT <<< $(echo "$TASKS_JSON" | /usr/bin/python3 -c "
import sys, json
from datetime import date
tasks = json.load(sys.stdin)
today = '$TODAY'
overdue = sum(1 for t in tasks if t.get('due') and t['due'] < today)
today_count = sum(1 for t in tasks if t.get('due') == today)
print(overdue, today_count)
" 2>/dev/null || echo "0 0")

# Menu bar display
if [ "$OVERDUE" -gt 0 ]; then
    echo "⚠ $OVERDUE | color=red"
elif [ "$TODAY_COUNT" -gt 0 ]; then
    echo "● $TODAY_COUNT | color=orange"
elif [ "$TOTAL" -gt 0 ]; then
    echo "○ $TOTAL"
else
    echo "✓"
fi

echo "---"

# Show tasks grouped
echo "$TASKS_JSON" | /usr/bin/python3 -c "
import sys, json
from datetime import date, datetime

tasks = json.load(sys.stdin)
today = '$TODAY'

# Group tasks
overdue = [t for t in tasks if t.get('due') and t['due'] < today]
due_today = [t for t in tasks if t.get('due') == today]
upcoming = [t for t in tasks if t.get('due') and t['due'] > today]
no_due = [t for t in tasks if not t.get('due')]

def print_task(t):
    title = t['title'][:40] + '...' if len(t['title']) > 40 else t['title']
    source = t.get('source', '')
    icon = '📁' if source == 'localfile' else '🔗'
    print(f'{icon} {title}')

if overdue:
    print(f'⚠️ Overdue ({len(overdue)}) | color=red')
    for t in overdue[:5]:
        print_task(t)
    if len(overdue) > 5:
        print(f'  ... and {len(overdue) - 5} more')
    print('---')

if due_today:
    print(f'📅 Today ({len(due_today)}) | color=orange')
    for t in due_today[:5]:
        print_task(t)
    if len(due_today) > 5:
        print(f'  ... and {len(due_today) - 5} more')
    print('---')

if upcoming:
    print(f'📆 Upcoming ({len(upcoming)})')
    for t in upcoming[:3]:
        due = t.get('due', '')
        print_task(t)
    if len(upcoming) > 3:
        print(f'  ... and {len(upcoming) - 3} more')
    print('---')

if no_due:
    print(f'📋 No Due Date ({len(no_due)})')
    for t in no_due[:3]:
        print_task(t)
    if len(no_due) > 3:
        print(f'  ... and {len(no_due) - 3} more')
" 2>/dev/null

echo "---"
echo "Open Dewey TUI | bash=$HOME/.cargo/bin/dewey param1=tui terminal=true"
echo "Refresh | refresh=true"
