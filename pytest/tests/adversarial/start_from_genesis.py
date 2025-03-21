import sys, time

sys.path.append('lib')

from cluster import start_cluster
from configured_logger import logger

overtake = False  # create a new chain which is shorter than current one
if "overtake" in sys.argv:
    overtake = True  # create a new chain which is longer than current one

doomslug = True
if "doomslug_off" in sys.argv:
    doomslug = False  # turn off doomslug

TIMEOUT = 300
BLOCKS = 30

# Low sync_check_period to sync from a new peer with greater height
client_config_change = {
    "consensus": {
        "sync_check_period": {
            "secs": 0,
            "nanos": 100000000
        }
    }
}

nodes = start_cluster(
    2, 0, 2, None,
    [["epoch_length", 100], ["block_producer_kickout_threshold", 80]],
    {0: client_config_change})
if not doomslug:
    # we expect inconsistency in store in node 0
    # because we're going to turn off doomslug
    # and allow applying blocks without proper validation
    nodes[0].stop_checking_store()

started = time.time()

time.sleep(2)
logger.info("Waiting for %s blocks..." % BLOCKS)

while True:
    assert time.time() - started < TIMEOUT
    status = nodes[0].get_status()
    height = status['sync_info']['latest_block_height']
    logger.info(status)
    if height >= BLOCKS:
        break
    time.sleep(1)

logger.info("Got to %s blocks, getting to fun stuff" % BLOCKS)

status = nodes[0].get_status()
logger.info(f"STATUS OF HONEST {status}")
saved_blocks = nodes[0].json_rpc('adv_get_saved_blocks', [])
logger.info(f"SAVED BLOCKS {saved_blocks}")

nodes[0].kill()  # to disallow syncing
nodes[1].kill()

# Switch node1 to an adversarial chain
nodes[1].reset_data()
nodes[1].start(nodes[0].node_key.pk, nodes[0].addr())

num_produce_blocks = BLOCKS // 2 - 5
if overtake:
    num_produce_blocks += 10

res = nodes[1].json_rpc('adv_produce_blocks', [num_produce_blocks, True])
assert 'result' in res, res
time.sleep(2)
nodes[1].kill()

# Restart both nodes.
# Disabling doomslug must happen before starting node1
nodes[0].start(nodes[0].node_key.pk, nodes[0].addr())
if not doomslug:
    res = nodes[0].json_rpc('adv_disable_doomslug', [])
    assert 'result' in res, res
nodes[1].start(nodes[0].node_key.pk, nodes[0].addr())

time.sleep(3)
status = nodes[1].get_status()
logger.info(f"STATUS OF MALICIOUS {status}")

status = nodes[0].get_status()
logger.info(f"STATUS OF HONEST AFTER {status}")
height = status['sync_info']['latest_block_height']

saved_blocks_2 = nodes[0].json_rpc('adv_get_saved_blocks', [])
logger.info(f"SAVED BLOCKS AFTER MALICIOUS INJECTION {saved_blocks_2}")
logger.info(f"HEIGHT {height}")

assert saved_blocks['result'] < BLOCKS + 10
if overtake and not doomslug:
    # node 0 should accept additional blocks from node 1 because of new chain is longer and doomslug is turned off
    assert saved_blocks_2['result'] >= BLOCKS + num_produce_blocks
else:
    assert saved_blocks_2['result'] < saved_blocks['result'] + 10

logger.info("Epic")
