#!/bin/bash

# Define the function to make the curl request and extract the height and db_hash from JSON
get_info() {
    local ip=$1
    local height=$(curl -s $ip/visa | jq -r '.db_status.height')
    local db_hash=$(curl -s $ip/visa | jq -r '.db_status.db_hash' | head -c 32 | xxd -p)
    echo "IP: $ip - Height: $height - DB Hash: $db_hash"
}

# Read each IP address from the file and call the function
while IFS= read -r ip || [[ -n "$ip" ]]; do
    get_info "$ip"
done < ip_addresses_local.txt

