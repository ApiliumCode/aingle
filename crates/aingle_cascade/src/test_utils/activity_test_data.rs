use ::fixt::prelude::*;
use ai_hash::AgentPubKey;
use ai_hash::EntryHash;
use ai_hash::HeaderHash;
use aingle_types::activity::ChainItems;
use aingle_types::sgd_op::SgdOp;
use aingle_types::sgd_op::SgdOpHashed;

use aingle_zome_types::*;

#[derive(Debug)]
pub struct ActivityTestData {
    pub hash_ops: Vec<SgdOpHashed>,
    pub noise_ops: Vec<SgdOpHashed>,
    pub store_ops: Vec<SgdOpHashed>,
    pub agent: AgentPubKey,
    pub query_filter: ChainQueryFilter,
    pub valid_hashes: ChainItems<HeaderHash>,
    pub valid_elements: ChainItems<Element>,
    pub chain_head: ChainHead,
    pub highest_observed: HighestObserved,
}

impl ActivityTestData {
    pub fn valid_chain_scenario() -> Self {
        // The agent we are querying.
        let agent = fixt!(AgentPubKey);

        // An entry that all headers can use to make things simpler.
        let entry = Entry::App(fixt!(AppEntryBytes));
        let entry_hash = EntryHash::with_data_sync(&entry);

        let to_op =
            |h| SgdOpHashed::from_content_sync(SgdOp::RegisterAgentActivity(fixt!(Signature), h));

        let to_element_and_op = |h: Header| {
            let sig = fixt!(Signature);
            // let e = Entry::App(fixt!(AppEntryBytes));
            let op = SgdOpHashed::from_content_sync(SgdOp::StoreElement(
                sig.clone(),
                h.clone(),
                Some(Box::new(entry.clone())),
            ));
            let shh = SignedHeaderHashed::with_presigned(HeaderHashed::from_content_sync(h), sig);
            (Element::new(shh, Some(entry.clone())), op)
        };

        let to_element_saf_op = |h: Header| {
            let sig = fixt!(Signature);
            let op =
                SgdOpHashed::from_content_sync(SgdOp::StoreElement(sig.clone(), h.clone(), None));
            let shh = SignedHeaderHashed::with_presigned(HeaderHashed::from_content_sync(h), sig);
            (Element::new(shh, None), op)
        };

        // The hashes we are expecting to get returned by the below activity set.
        let mut valid_hashes = Vec::new();

        // The elements on the chain. Needs to match the activity set.
        let mut valid_elements = Vec::new();

        // The store element ops for the actual data on the chain which should
        // match the set of activity ops.
        let mut store_ops = Vec::new();

        // A set of activity ops:
        // - Must be on the above agents chain.
        // - Create a valid, unbroken chain.
        // - All headers are valid:
        //    - Prev hash actually match prev header's hash
        //    - Seq numbers are in order.
        //    - First header must be a Saf.
        let mut hash_ops = Vec::new();
        let mut saf = fixt!(Saf);
        saf.author = agent.clone();
        let saf = Header::Saf(saf);

        // Insert the saf
        let (el, op) = to_element_saf_op(saf.clone());
        valid_elements.push(el);
        store_ops.push(op);
        hash_ops.push(to_op(saf.clone()));

        let creates: Vec<_> = CreateFixturator::new(Unpredictable)
            .enumerate()
            .take(50)
            .collect();
        let mut prev_hash = HeaderHash::with_data_sync(&saf);
        valid_hashes.push((0, prev_hash.clone()));
        for (seq, mut create) in creates {
            let header_seq = (seq + 1) as u32;
            create.author = agent.clone();
            create.header_seq = header_seq;
            create.prev_header = prev_hash.clone();
            create.entry_hash = entry_hash.clone();
            let header = Header::Create(create);
            prev_hash = HeaderHash::with_data_sync(&header);
            hash_ops.push(to_op(header.clone()));

            valid_hashes.push((header_seq, prev_hash.clone()));

            let (el, op) = to_element_and_op(header);
            valid_elements.push(el);
            store_ops.push(op);
        }

        // The head of the chain is the last valid hash
        // because we are going to insert all ops as valid and integrated.
        let last = valid_hashes.last().unwrap();
        let chain_head = ChainHead {
            header_seq: last.0,
            hash: last.1.clone(),
        };

        // Highest Observed is the same as the chain head.
        let highest_observed = HighestObserved {
            header_seq: last.0,
            hash: vec![last.1.clone()],
        };

        // We just want a simple query filter to get back the full chain.
        let query_filter = QueryFilter::new();

        // Finally add some random noise so we know we are getting the correct items.
        let noise_ops = HeaderFixturator::new(Unpredictable)
            .take(50)
            .map(to_op)
            .collect();

        Self {
            hash_ops,
            agent,
            query_filter,
            valid_hashes: ChainItems::Hashes(valid_hashes),
            highest_observed,
            chain_head,
            noise_ops,
            store_ops,
            valid_elements: ChainItems::Full(valid_elements),
        }
    }
}
