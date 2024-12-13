from pyulog import ULog
from pprint import pprint
import json

test_files = [
    'test_data/sample.ulg',  # v0 ulog
    'test_data/sample-small.ulg',  # v1 ulog 
]
def topics_and_counts(dl):
    return {k: len(dl.data[k]) for k,v in dl.data.items()}

# Get list of all datasets, topics, row counts, etc.
def profiler():
    for file in test_files:
        file_name = file.split('/')[-1]
        u = ULog(file)
        profile = {
            'file': file_name,
            # Datasets
            'datasets': {ds.name: topics_and_counts(ds) for ds in u.data_list},
            # Messages
            'logged_messages': [lm.__dict__ for lm in u.logged_messages],
            # 'logged_messages_tagged': {},
            'info_messages': u.msg_info_dict,
            # 'multi_messages': u.msg_info_multiple_dict,
            # Params
            'default_params': u._default_parameters,
            'initial_params': u.initial_parameters,
            'changed_params': u.changed_parameters,
            # Dropouts
            'dropout_details': [
                {'timestamp': 0, 'duration': 0} for do in u.dropouts
            ],
        }
        pprint(profile)
        # Write the file out to /parity/results/<file>.py.json
        # Create if it doesn't exist
        with open(f'parity/results/{file_name}.py.json', 'w') as f:
            json.dump(profile, f, indent=2)
profiler()
