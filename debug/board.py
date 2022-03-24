#!/usr/bin/env python

import json
import base64

board_id = int(input("Введите id доски: "))

task = {
  "board_id": board_id
}
s = json.dumps(task)
token = base64.b64encode(bytes(s, 'utf-8'))

print(token)
