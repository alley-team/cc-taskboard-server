#!/usr/bin/env python

import json
import base64

task = { "key": input("Введите ключ администратора: ") }

s = json.dumps(task)
token = base64.b64encode(bytes(s, 'utf-8'))

print(token)
