rule star_genome_generate:
    input:
        fasta="workflow/results/reference/GRCh38.primary_assembly.genome.fa.gz",
        gtf="workflow/results/reference/gencode.v{release}.annotation.gtf.gz".format(
            release=config["gencode_release"]
        ),
    output:
        directory("workflow/results/reference/star_index"),
    log:
        "workflow/results/logs/star_genome_generate.log",
    threads: config["star"]["threads"]
    shell:
        """
        mkdir -p {output}
        STAR --runMode genomeGenerate \
            --runThreadN {threads} \
            --genomeDir {output} \
            --genomeFastaFiles {input.fasta} \
            --readFilesCommand zcat \
            --sjdbGTFfile {input.gtf} \
            --sjdbOverhang {config[star][sjdb_overhang]} \
            2> {log}
        """
