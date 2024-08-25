//// CHANGE name=change0
CREATE TABLE table_generated_id (
    gen_id integer NOT NULL,
    field1 integer
);



GO

//// CHANGE name=change1
ALTER TABLE ONLY table_generated_id ALTER COLUMN gen_id SET DEFAULT nextval('table_generated_id_gen_id_seq'::regclass);



GO
