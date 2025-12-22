use aingle_sqlite::db::WriteManager;
use aingle_state::mutations;
use aingle_state::prelude::test_cell_env;
use aingle_types::sgd_op::SgdOpLight;
use aingle_types::Timestamp;
use aingle_zome_types::fixt::*;
use aingle_zome_types::ValidationStatus;
use chrono::Duration;
use chrono::Utc;
use fixt::prelude::*;

#[tokio::test(flavor = "multi_thread")]
async fn test_sgd_op_query() {
    let test_env = test_cell_env();
    let env = test_env.env();

    // Create some integration values
    let mut expected = Vec::new();
    let mut basis = AnySgdHashFixturator::new(Predictable);
    let now = Utc::now();
    let same_basis = basis.next().unwrap();
    let mut times = Vec::new();
    times.push(now - Duration::hours(100));
    times.push(now);
    times.push(now + Duration::hours(100));
    let times_exp = times.clone();
    let values = times.into_iter().map(|when_integrated| {
        (
            ValidationStatus::Valid,
            SgdOpLight::RegisterAgentActivity(fixt!(HeaderHash), basis.next().unwrap()),
            Timestamp::from(when_integrated),
        )
    });

    // Put them in the db
    {
        let mut sgd_hash = SgdOpHashFixturator::new(Predictable);
        for (validation_status, op, when_integrated) in values {
            env.conn()
                .unwrap()
                .with_commit(|txn| mutations::insert_op())
                .unwrap();
            buf.put(sgd_hash.next().unwrap(), value.clone()).unwrap();
            expected.push(value.clone());
            value.op = SgdOpLight::RegisterAgentActivity(fixt!(HeaderHash), same_basis.clone());
            buf.put(sgd_hash.next().unwrap(), value.clone()).unwrap();
            expected.push(value.clone());
        }
    }

    // Check queries

    let mut conn = env.conn().unwrap();
    conn.with_reader_test(|mut reader| {
        let buf = IntegratedSgdOpsBuf::new(env.clone().into()).unwrap();
        // No filter
        let mut r = buf
            .query(&mut reader, None, None, None)
            .unwrap()
            .map(|(_, v)| Ok(v))
            .collect::<Vec<_>>()
            .unwrap();
        r.sort_by_key(|v| v.when_integrated.clone());
        assert_eq!(&mut r[..], &expected[..]);
        // From now
        let mut r = buf
            .query(&mut reader, Some(times_exp[1].clone().into()), None, None)
            .unwrap()
            .map(|(_, v)| Ok(v))
            .collect::<Vec<_>>()
            .unwrap();
        r.sort_by_key(|v| v.when_integrated.clone());
        assert!(r.contains(&expected[2]));
        assert!(r.contains(&expected[4]));
        assert!(r.contains(&expected[3]));
        assert!(r.contains(&expected[5]));
        assert_eq!(r.len(), 4);
        // From ages ago till 1hr in future
        let ages_ago = times_exp[0] - Duration::weeks(5);
        let future = times_exp[1] + Duration::hours(1);
        let mut r = buf
            .query(
                &mut reader,
                Some(ages_ago.into()),
                Some(future.into()),
                None,
            )
            .unwrap()
            .map(|(_, v)| Ok(v))
            .collect::<Vec<_>>()
            .unwrap();
        r.sort_by_key(|v| v.when_integrated.clone());

        assert!(r.contains(&expected[0]));
        assert!(r.contains(&expected[1]));
        assert!(r.contains(&expected[2]));
        assert!(r.contains(&expected[3]));
        assert_eq!(r.len(), 4);
        // Same basis
        let ages_ago = times_exp[0] - Duration::weeks(5);
        let future = times_exp[1] + Duration::hours(1);
        let mut r = buf
            .query(
                &mut reader,
                Some(ages_ago.into()),
                Some(future.into()),
                Some(SgdArc::new(same_basis.get_loc(), 1)),
            )
            .unwrap()
            .map(|(_, v)| Ok(v))
            .collect::<Vec<_>>()
            .unwrap();
        r.sort_by_key(|v| v.when_integrated.clone());
        assert!(r.contains(&expected[1]));
        assert!(r.contains(&expected[3]));
        assert_eq!(r.len(), 2);
        // Same basis all
        let mut r = buf
            .query(
                &mut reader,
                None,
                None,
                Some(SgdArc::new(same_basis.get_loc(), 1)),
            )
            .unwrap()
            .map(|(_, v)| Ok(v))
            .collect::<Vec<_>>()
            .unwrap();
        r.sort_by_key(|v| v.when_integrated.clone());
        assert!(r.contains(&expected[1]));
        assert!(r.contains(&expected[3]));
        assert!(r.contains(&expected[5]));
        assert_eq!(r.len(), 3);
    });
}
